use janus_core::{availability, cases, db, dedup, doctor, export, fetch, identity, parse, profile, query, radar, scan, search, store, writer};
use rusqlite::{Connection, params};
use std::io::BufRead;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print!("{}", janus::usage::first_run());
        return;
    }
    if matches!(args[1].as_str(), "--help" | "-h" | "help") {
        print!("{}", janus::usage::help());
        return;
    }
    if args[1] == "daemon" {
        std::process::exit(cmd_daemon(&args[2..]));
    }
    if writes(&args[1], &args[2..]) {
        if let Some(url) = writer::live_daemon() {
            eprintln!("daemon is the SQLite writer at {url} — use the UI or stop the daemon");
            std::process::exit(1);
        }
    }
    let conn = open_db();
    let code = match args[1].as_str() {
        "db" => cmd_db(&conn, &args[2..]),
        "root" => cmd_root(&conn, &args[2..]),
        "scan" => cmd_scan(&conn, &args[2..]),
        "status" => cmd_status(&conn, &args[2..]),
        "list" => cmd_list(&conn, &args[2..]),
        "show" => cmd_show(&conn, &args[2..]),
        "identify" => cmd_identify(&conn, &args[2..]),
        "search" => cmd_search(&conn, &args[2..]),
        "merge" => cmd_merge(&conn, &args[2..]),
        "dedup" => cmd_dedup(&conn, &args[2..]),
        "storage" => cmd_storage(&conn, &args[2..]),
        "cold" => cmd_cold(&conn, &args[2..]),
        "doctor" => cmd_doctor(&conn, &args[2..]),
        "decline" => cmd_decline(&conn, &args[2..]),
        "export" => cmd_export(&conn, &args[2..]),
        "import" => cmd_import(&conn, &args[2..]),
        "cases" => cmd_cases(&args[2..]),
        "have" => cmd_have(&conn, &args[2..]),
        "profile" => cmd_profile(&conn, &args[2..]),
        "monitor" => cmd_monitor(&conn, &args[2..]),
        "radar" => cmd_radar(&conn, &args[2..]),
        "wanted" => cmd_wanted(&conn, &args[2..]),
        "fetch" => cmd_fetch(&conn, &args[2..]),
        "verify" => cmd_verify(&conn, &args[2..]),
        _ => {
            print!("{}", janus::usage::help());
            2
        }
    };
    std::process::exit(code);
}

fn writes(cmd: &str, rest: &[String]) -> bool {
    match cmd {
        "scan" | "identify" | "merge" | "import" | "cold" | "decline" | "radar" | "fetch" | "verify" => true,
        "root" => !matches!(rest.first().map(|s| s.as_str()), Some("ls") | None),
        "profile" => matches!(rest.first().map(|s| s.as_str()), Some("set")),
        "monitor" => !matches!(rest.first().map(|s| s.as_str()), Some("ls") | None),
        _ => false,
    }
}

fn cmd_daemon(args: &[String]) -> i32 {
    let mut api = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--api" {
            api = args.get(i + 1).cloned();
            i += 2;
        } else {
            i += 1;
        }
    }
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    match rt.block_on(janus::daemon::run(api.as_deref())) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn wants_json(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

fn fail(code: &str) -> i32 {
    eprintln!("{code}");
    1
}

fn open_db() -> Connection {
    let p = store::db_path();
    let conn = db::open(Some(&p)).expect("open db");
    db::require_schema(&conn).expect("schema");
    conn
}

fn resolve_root(conn: &Connection, s: &str) -> Option<i64> {
    if let Ok(id) = s.parse::<i64>() {
        if store::root_by_id(conn, id).is_ok() {
            return Some(id);
        }
    }
    store::root_ls(conn)
        .ok()?
        .into_iter()
        .find(|r| r.name == s || r.path == s)
        .map(|r| r.id)
}

fn cmd_db(conn: &Connection, _args: &[String]) -> i32 {
    println!("db: {}", store::db_path().display());
    println!("cache: {}", store::cache_dir().display());
    let version = db::schema_version(conn).unwrap_or_else(|_| "?".into());
    let algo = db::family_key_algo(conn).unwrap_or_else(|_| "?".into());
    println!("schema_version: {version}");
    println!("family_key_algo: {algo}");
    let (all, present) = store::present_count(conn).unwrap_or((0, 0));
    println!("roots: {present}/{all} present");
    0
}

fn cmd_root(conn: &Connection, args: &[String]) -> i32 {
    if args.is_empty() || args[0] == "ls" {
        let json = wants_json(args);
        let rows = store::root_ls(conn).unwrap_or_default();
        if json {
            match query::roots(conn) {
                Ok(v) => {
                    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                    return 0;
                }
                Err(e) => return fail(&e),
            }
        }
        println!("{:<4} {:<12} {:<12} {:<8} {:<6} PATH", "ID", "NAME", "KIND", "PRESENT", "COLD");
        for r in rows {
            println!(
                "{:<4} {:<12} {:<12} {:<8} {:<6} {}",
                r.id,
                r.name,
                r.kind,
                if r.present.unwrap_or(0) == 1 { "yes" } else { "no" },
                if r.cold == 1 { "yes" } else { "no" },
                r.path
            );
        }
        return 0;
    }
    match args[0].as_str() {
        "add" => {
            let mut kind = "internal".to_string();
            let mut name: Option<String> = None;
            let mut cold = false;
            let mut accept_marker = false;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--kind" => {
                        kind = args.get(i + 1).cloned().unwrap_or_default();
                        i += 2;
                    }
                    "--name" => {
                        name = args.get(i + 1).cloned();
                        i += 2;
                    }
                    "--cold" => {
                        cold = true;
                        i += 1;
                    }
                    "--accept-marker" => {
                        accept_marker = true;
                        i += 1;
                    }
                    _ => break,
                }
            }
            let path = args.get(i).cloned().unwrap_or_default();
            if path.is_empty() {
                println!("root add: missing PATH");
                return 2;
            }
            let name = name.unwrap_or_else(|| Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(path.clone()));
            match store::root_add_opts(conn, &name, &path, &kind, accept_marker) {
                Ok(id) => {
                    if cold {
                        if let Err(e) = store::root_set_cold(conn, id, true) {
                            println!("error: {e}");
                            return 1;
                        }
                    }
                    println!("added root {id} {name} {path} kind={kind} cold={cold}");
                    0
                }
                Err(e) => {
                    if e == "root.no_mount_id" {
                        eprintln!("root.no_mount_id — volume has no UUID/serial. Re-run with --accept-marker to write .janus-root (opt-in).");
                    }
                    fail(&e)
                }
            }
        }
        "discover" => match store::discover_roots(conn) {
            Ok(ids) => {
                if ids.is_empty() {
                    println!("no discovery roots found (Ollama / LM Studio / HF cache)");
                } else {
                    println!("added discovery roots: {ids:?}");
                }
                0
            }
            Err(e) => fail(&e),
        },
        "probe" => {
            let Some(raw) = args.get(1) else {
                println!("root probe: missing id");
                return 2;
            };
            let Some(id) = resolve_root(conn, raw) else {
                return fail("root.not_found");
            };
            match store::root_by_id(conn, id) {
                Ok(root) => {
                    if root.cold == 1 {
                        println!("{} {} cold=yes (not polled; run scan to refresh)", root.id, root.path);
                    }
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let present = store::root_probe(conn, &root, now);
                    println!("{} present={}", root.path, present);
                    0
                }
                Err(e) => {
                    println!("error: {e}");
                    1
                }
            }
        }
        "rm" => {
            let Some(raw) = args.get(1) else {
                println!("root rm: missing id");
                return 2;
            };
            let id: i64 = match raw.parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    println!("root rm: invalid id");
                    return 2;
                }
            };
            match store::root_rm(conn, id) {
                Ok(()) => {
                    println!("removed root {id}");
                    0
                }
                Err(e) => fail(&e),
            }
        }
        _ => {
            print!("{}", janus::usage::help());
            2
        }
    }
}

fn cmd_scan(conn: &Connection, args: &[String]) -> i32 {
    let quick = args.iter().any(|a| a == "--quick");
    let target = args.iter().find(|a| !a.starts_with('-')).cloned().unwrap_or_default();
    let ids: Vec<i64> = if target.is_empty() {
        store::root_ls(conn)
            .unwrap_or_default()
            .into_iter()
            .filter(|r| std::path::Path::new(&r.path).is_dir())
            .map(|r| r.id)
            .collect()
    } else {
        match resolve_root(conn, &target) {
            Some(id) => vec![id],
            None => {
                eprintln!("scan: unknown root {target}");
                return 2;
            }
        }
    };
    if ids.is_empty() {
        println!("scan: no present roots (run: janus root add PATH)");
        return 0;
    }
    let opts = scan::ScanOptions { quick };
    let mut code = 0;
    for root_id in ids {
        match scan::scan_root(conn, root_id, &opts) {
            Ok(rep) => {
                let mode = if quick { "quick" } else { "full" };
                println!(
                    "{mode} scan root {root_id}: seen={} new={} changed={} unsupported={} unverified={} families_new={} symlink_dirs_skipped={} dirs_unreadable={} non_utf8={} deep={} root_offline={}",
                    rep.files_seen, rep.files_new, rep.files_changed, rep.files_unsupported, rep.files_unverified, rep.families_new, rep.skipped_symlink_dirs, rep.dirs_unreadable, rep.skipped_non_utf8, rep.skipped_deep, rep.root_offline
                );
            }
            Err(e) => {
                eprintln!("{e}");
                code = 1;
            }
        }
    }
    code
}

fn cmd_status(conn: &Connection, args: &[String]) -> i32 {
    match query::home(conn) {
        Ok(h) => {
            if wants_json(args) {
                println!("{}", serde_json::to_string_pretty(&h).unwrap_or_default());
                return 0;
            }
            println!(
                "families: {} ({} name-inferred)",
                h.counts.families, h.counts.families_inferred
            );
            println!("bytes indexed: {}", h.counts.bytes);
            println!("files not full-hashed: {}", h.counts.unverified);
            println!("filenames with content role unattached: {}", h.counts.unknown_files);
            println!("roots: {}/{} present", h.counts.roots_present, h.counts.roots);
            0
        }
        Err(e) => fail(&e),
    }
}

fn cmd_list(conn: &Connection, args: &[String]) -> i32 {
    let list = match query::models(conn, &query::ModelFilter::default()) {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    if wants_json(args) {
        println!("{}", serde_json::to_string_pretty(&list).unwrap_or_default());
        return 0;
    }
    let inferred = list.counts.families_inferred;
    let known = list.counts.families.saturating_sub(inferred);
    println!(
        "{} families ({} known/manual, {} inferred)",
        list.counts.families, known, inferred
    );
    if list.families.is_empty() {
        println!("no families (run: janus root add PATH && janus scan && janus list)");
        return 0;
    }
    println!("{:<28} {:<5} {:<9} {:<22} {:<10} ROOTS", "FAMILY", "KIND", "PARAMS", "VARIANTS", "SIZE");
    for f in &list.families {
        let raw = f
            .name
            .value
            .as_deref()
            .unwrap_or(f.family_key.split('|').next().unwrap_or("unknown"));
        let name = if f.name.level == "inferred" {
            format!("{raw}~")
        } else {
            raw.to_string()
        };
        let params = identity::format_params_b(f.params_total);
        let roots: Vec<String> = f
            .roots
            .iter()
            .map(|r| if r.present { r.name.clone() } else { format!("[{}]", r.name) })
            .collect();
        let roots = roots.join(",");
        let roots = if roots.is_empty() { "—".to_string() } else { roots };
        println!(
            "{:<28} {:<5} {:<9} {:<22} {:<10} {}",
            name,
            f.kind.value,
            params,
            short_quants(&f.quants),
            human_bytes(f.bytes),
            roots
        );
    }
    0
}

fn short_quants(q: &str) -> String {
    let parts: Vec<&str> = q.split(',').collect();
    match parts.len() {
        0 => String::new(),
        1 => parts[0].to_string(),
        n => format!("{} +{}", parts[0], n - 1),
    }
}

fn human_bytes(b: i64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    if b <= 0 {
        return "0B".to_string();
    }
    let mut v = b as f64;
    let mut u = 0usize;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1}{}", UNITS[u])
}

fn cmd_show(conn: &Connection, args: &[String]) -> i32 {
    let Some(q) = args.first() else {
        println!("show: need a family name or key");
        return 2;
    };
    let Some(fid) = store::family_find_id(conn, q) else {
        println!("show: no family matches {q}");
        return 1;
    };
    if wants_json(args) {
        match query::model(conn, fid) {
            Ok(v) => {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                return 0;
            }
            Err(e) => return fail(&e),
        }
    }
    let Some(f) = store::family_list(conn).ok().and_then(|v| v.into_iter().find(|x| x.id == fid)) else {
        println!("show: family vanished");
        return 1;
    };
    let name = f.name.as_deref().unwrap_or("(no name)");
    let level = f.name_level.as_deref().unwrap_or("•");
    println!("Family  {name}");
    println!("  key={}", f.key);
    println!("  kind={} (name level: {})", f.kind, level);
    if let Some(t) = f.params_total {
        println!("  params_total={t}");
    }
    let variants = match store::family_variants(conn, fid) {
        Ok(v) => v,
        Err(e) => {
            println!("error: {e}");
            return 1;
        }
    };
    println!("Variants:");
    if variants.is_empty() {
        println!("  (none)");
    }
    for v in &variants {
        let last = v
            .last_file_mtime
            .map(|t| format!("last seen {}", fmt_time(t)))
            .unwrap_or_default();
        let pres = if v.present { "present" } else { "offline" };
        println!(
            "  {:<8} {:<6} {:<12} {:<12} {:<10} {} {}",
            v.quant,
            v.format,
            v.subflavour,
            v.publisher,
            human_bytes(v.bytes),
            if v.present { v.root.clone() } else { format!("[{}] {}", v.root, last) },
            pres
        );
    }
    0
}

fn fmt_time(t: i64) -> String {
    let d = std::time::Duration::from_secs(t.max(0) as u64);
    let dt = std::time::UNIX_EPOCH + d;
    let s = format!("{:?}", dt).replace("SystemTime { tv_sec: ", "").replace(" }", "");
    s.chars().take(19).collect()
}

fn cmd_identify(conn: &Connection, args: &[String]) -> i32 {
    let non_interactive = args.iter().any(|a| a == "--non-interactive");
    let mut name_flag: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--name" {
            name_flag = args.get(i + 1).cloned();
            i += 2;
        } else {
            i += 1;
        }
    }
    let path = args
        .iter()
        .enumerate()
        .find_map(|(i, a)| {
            if a.starts_with('-') {
                return None;
            }
            if i > 0 && args[i - 1] == "--name" {
                return None;
            }
            Some(a.as_str())
        })
        .unwrap_or_default();
    if path.is_empty() {
        println!("identify: need FILE");
        return 2;
    }
    let p = Path::new(path);
    let format = match janus_core::detect::detect(p) {
        Ok(f) => f,
        Err(e) => {
            println!("identify: cannot read {path}: {e}");
            return 1;
        }
    };
    let parsed = parse::parse_prefix(p, &format, scan::GGUF_PREFIX_CAP);
    let cand = identity::identify(path, &parsed);
    println!("path: {path}");
    println!("format: {:?}", format);
    if let Some(e) = &parsed.parse_error {
        println!("parse_error: {e}");
    }
    println!("role: {:?}", cand.role);
    println!("display_name: {}", cand.display_name.value);
    if let Some(k) = &cand.family_key {
        println!("family_key: {k}");
    }
    let is_unknown = cand.is_unknown;
    println!("is_unknown: {is_unknown}");
    println!("quant: {} (from {})", cand.quant.value, kind_of(&cand.quant.level));
    println!("subflavour: {}", cand.subflavour.value);
    println!("publisher: {}", cand.publisher.value);
    match janus_core::hash::full_hash(p) {
        Ok((b3, s256, size, _partial)) => {
            println!("sha256: {s256}");
            println!("blake3: {b3}");
            println!("size: {}", human_bytes(size as i64));
        }
        Err(e) => println!("hash: {e}"),
    }
    let mut chosen = name_flag.unwrap_or_default();
    if is_unknown && chosen.is_empty() && !non_interactive {
        println!("---");
        println!("Name this file? (empty to skip)");
        chosen = std::io::stdin().lock().lines().next().and_then(|l| l.ok()).unwrap_or_default();
        chosen = chosen.trim().to_string();
    }
    if is_unknown && !chosen.trim().is_empty() {
        match store::persist_manual_name(conn, p, chosen.trim()) {
            Ok(id) => println!("named '{chosen}' as manual (family id {id})"),
            Err(e) => {
                eprintln!("{e}");
                if e == "root.not_found" {
                    println!("file is not under a known root; name not persisted");
                }
            }
        }
    }
    0
}

fn kind_of(l: &janus_core::ev::Level) -> &'static str {
    match l {
        janus_core::ev::Level::Known => "header",
        janus_core::ev::Level::Inferred => "filename",
        janus_core::ev::Level::Detected => "structure",
        janus_core::ev::Level::External => "external",
        janus_core::ev::Level::Manual => "user",
    }
}

fn cmd_search(conn: &Connection, args: &[String]) -> i32 {
    let q = args.iter().filter(|a| !a.starts_with('-')).cloned().collect::<Vec<_>>().join(" ");
    if q.is_empty() {
        println!("search: need QUERY");
        return 2;
    }
    if wants_json(args) {
        match query::search_json(conn, &q) {
            Ok(v) => {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                return 0;
            }
            Err(e) => return fail(&e),
        }
    }
    match search::search(conn, &q) {
        Ok(hits) => {
            if hits.is_empty() {
                println!("no matches");
                return 0;
            }
            println!("{:<8} {:<28} KEY/PATH", "KIND", "NAME");
            for h in hits {
                let extra = h.path.or(h.key).unwrap_or_default();
                let extra = if h.present { extra } else { format!("[{extra}]") };
                println!("{:<8} {:<28} {extra}", h.kind, h.name);
            }
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_merge(conn: &Connection, args: &[String]) -> i32 {
    if args.first().map(|s| s.as_str()) == Some("--decline") {
        return cmd_decline(conn, &args[1..]);
    }
    if args.len() < 2 {
        println!("merge: need SRC TARGET");
        return 2;
    }
    match store::merge_families(conn, &args[0], &args[1]) {
        Ok(id) => {
            println!("merged {} -> {} (family id {id})", args[0], args[1]);
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_storage(conn: &Connection, args: &[String]) -> i32 {
    match store::storage_summary(conn) {
        Ok(rows) => {
            if wants_json(args) {
                match query::storage(conn) {
                    Ok(v) => {
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                        return 0;
                    }
                    Err(e) => return fail(&e),
                }
            }
            if rows.is_empty() {
                println!("no roots");
                return 0;
            }
            println!("{:<4} {:<12} {:<10} {:<8} {:<6} {:<6} {:<10} RECLAIMABLE", "ID", "NAME", "KIND", "PRESENT", "COLD", "FILES", "BYTES");
            for r in rows {
                println!(
                    "{:<4} {:<12} {:<10} {:<8} {:<6} {:<6} {:<10} {}",
                    r.id,
                    r.name,
                    r.kind,
                    if r.present { "yes" } else { "no" },
                    if r.cold { "yes" } else { "no" },
                    r.files,
                    human_bytes(r.bytes),
                    if r.present { human_bytes(r.reclaimable) } else { "0 (offline)".into() }
                );
            }
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_cold(conn: &Connection, args: &[String]) -> i32 {
    let Some(action) = args.first() else {
        println!("cold: mark|unmark ID");
        return 2;
    };
    let Some(raw) = args.get(1) else {
        println!("cold: missing id");
        return 2;
    };
    let Some(id) = resolve_root(conn, raw) else {
        println!("error: root.not_found");
        return 1;
    };
    let cold = match action.as_str() {
        "mark" => true,
        "unmark" => false,
        _ => {
            println!("cold: mark|unmark ID");
            return 2;
        }
    };
    match store::root_set_cold(conn, id, cold) {
        Ok(()) => {
            println!("root {id} cold={cold}");
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_import(conn: &Connection, args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        println!("import: need PATH");
        return 2;
    };
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            println!("error: {e}");
            return 1;
        }
    };
    let v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            println!("error: export.incomplete ({e})");
            return 1;
        }
    };
    match export::import(conn, &v) {
        Ok(r) => {
            println!("imported families={} aliases={} declined={}", r.families, r.aliases, r.declined);
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_dedup(conn: &Connection, args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--apply") {
        eprintln!("dedup --apply is later; this build is report-only (dedup --plan)");
        return 2;
    }
    if wants_json(args) {
        match query::dups(conn) {
            Ok(v) => {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                return 0;
            }
            Err(e) => return fail(&e),
        }
    }
    let groups = dedup::plan(conn);
    let mut files: i64 = 0;
    let mut bytes: i64 = 0;
    for g in &groups {
        files += g.reclaimable_files;
        bytes += g.reclaimable_bytes;
    }
    println!("duplicate groups: {}", groups.len());
    println!("reclaimable files: {files}  reclaimable bytes: {bytes}  (unique mount_id,dev,ino)");
    for g in &groups {
        println!("{}  size={} allocations={} reclaimable={} copies={}", g.blake3, g.size, g.allocations, g.reclaimable_files, g.copies.len());
        for c in &g.copies {
            let mount = c.mount_id.as_deref().unwrap_or("?");
            println!("    {} {} mount={mount} ino={}", c.root_name, c.rel_path, c.ino);
        }
    }
    0
}

fn cmd_verify(conn: &Connection, args: &[String]) -> i32 {
    let Some(target) = args.iter().find(|a| !a.starts_with('-')) else {
        println!("verify: need FILE or file id");
        return 2;
    };
    let path = if let Ok(id) = target.parse::<i64>() {
        match store::file_abs_path(conn, id) {
            Ok(p) => p,
            Err(e) => return fail(&e),
        }
    } else {
        PathBuf::from(target)
    };
    match janus_core::hash::full_hash(&path) {
        Ok((b3, s256, size, _)) => {
            match store::blob_upsert(conn, &b3, Some(&s256), size as i64, None) {
                Ok(blob_id) => {
                    println!("blake3={b3}");
                    println!("sha256={s256}");
                    println!("size={size}");
                    println!("blob_id={blob_id}");
                    0
                }
                Err(e) => fail(&e),
            }
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn cmd_doctor(conn: &Connection, args: &[String]) -> i32 {
    let rep = doctor::report(conn);
    if wants_json(args) {
        println!(
            "{}",
            serde_json::json!({
                "findings": rep.findings.iter().map(|f| serde_json::json!({"code": f.code, "count": f.count, "message": f.message})).collect::<Vec<_>>(),
                "suggestions": rep.suggestions.iter().map(|s| serde_json::json!({"a": s.a_key, "b": s.b_key, "reason": s.reason, "score": s.score})).collect::<Vec<_>>(),
            })
        );
        return 0;
    }
    if rep.findings.is_empty() {
        println!("no issues");
    }
    for f in &rep.findings {
        println!("{} count={}  {}", f.code, f.count, f.message);
    }
    if rep.suggestions.is_empty() {
        println!("no merge suggestions");
    }
    for s in &rep.suggestions {
        println!("suggest merge ({}): {} <-> {}  shared_tokens={} score={:.2}", s.reason, s.a_key, s.b_key, s.shared_tokens, s.score);
    }
    0
}

fn cmd_decline(conn: &Connection, args: &[String]) -> i32 {
    if args.len() < 2 {
        println!("decline: need two family keys");
        return 2;
    }
    match store::declined_merge(conn, &args[0], &args[1], janus_core::FAMILY_KEY_ALGO) {
        Ok(()) => {
            println!("declined merge between {} and {}", args[0], args[1]);
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_export(conn: &Connection, args: &[String]) -> i32 {
    let path = args.first().map(|s| s.as_str()).unwrap_or("-");
    match export::export(conn) {
        Ok(v) => {
            let out = serde_json::to_string_pretty(&v).unwrap_or_default();
            if path == "-" {
                println!("{out}");
            } else {
                if let Err(e) = std::fs::write(path, out) {
                    println!("error: {e}");
                    return 1;
                }
            }
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_cases(args: &[String]) -> i32 {
    use janus_core::cases::Status;
    let fixtures = match args.first() {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures"),
    };
    let mut pass = 0;
    let mut fail = 0;
    let mut skipped = 0;
    for c in cases::run_all(&fixtures) {
        match c.status {
            Status::Pass => pass += 1,
            Status::Fail => fail += 1,
            Status::Skipped => skipped += 1,
        }
        println!("{} {}: {:?}  {}", c.id, c.title, c.status, c.detail);
    }
    println!("\npass={pass} fail={fail} skipped={skipped}");
    if fail > 0 {
        1
    } else {
        0
    }
}

fn cmd_have(conn: &Connection, args: &[String]) -> i32 {
    let mut rel = String::new();
    let mut root_id = 0i64;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--root" {
            root_id = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
            i += 2;
        } else {
            if rel.is_empty() && !args[i].starts_with('-') {
                rel = args[i].clone();
            }
            i += 1;
        }
    }
    if rel.is_empty() {
        println!("have: missing rel_path");
        return 2;
    }
    let file_id: i64 = conn
        .query_row(
            "SELECT id FROM files WHERE root_id=?1 AND rel_path=?2",
            params![root_id, rel],
            |r| r.get(0),
        )
        .unwrap_or(-1);
    if file_id < 0 {
        println!("have: file not found");
        return 2;
    }
    println!("have_bytes: {}", dedup::have_bytes(conn, file_id));
    0
}

fn cmd_profile(conn: &Connection, args: &[String]) -> i32 {
    let json = wants_json(args);
    match args.first().map(|s| s.as_str()).unwrap_or("ls") {
        "ls" => {
            if let Err(e) = profile::ensure_default(conn) {
                return fail(&e);
            }
            match query::profiles(conn) {
                Ok(rows) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
                        return 0;
                    }
                    println!("{:<4} {:<16} CUTOFF     PUBLISHERS", "ID", "NAME");
                    for r in rows {
                        println!(
                            "{:<4} {:<16} {:<10} {}",
                            r.id,
                            r.spec.name,
                            r.spec.cutoff.as_deref().unwrap_or("—"),
                            r.spec.publishers.join(",")
                        );
                    }
                    0
                }
                Err(e) => fail(&e),
            }
        }
        "show" => {
            let name = args.get(1).cloned().unwrap_or_else(|| "daily-llm".into());
            match profile::get_by_name(conn, &name) {
                Ok(row) => {
                    println!("{}", serde_json::to_string_pretty(&row).unwrap_or_default());
                    0
                }
                Err(e) => fail(&e),
            }
        }
        "set" => {
            let mut spec = profile::default_daily_llm();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--name" => {
                        spec.name = args.get(i + 1).cloned().unwrap_or_default();
                        i += 2;
                    }
                    "--cutoff" => {
                        spec.cutoff = args.get(i + 1).cloned();
                        i += 2;
                    }
                    "--quants" => {
                        spec.quants = args
                            .get(i + 1)
                            .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                            .unwrap_or_default();
                        i += 2;
                    }
                    "--publishers" => {
                        spec.publishers = args
                            .get(i + 1)
                            .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                            .unwrap_or_default();
                        i += 2;
                    }
                    "--formats" => {
                        spec.formats = args
                            .get(i + 1)
                            .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                            .unwrap_or_default();
                        i += 2;
                    }
                    "--max-bytes" => {
                        spec.max_bytes = args.get(i + 1).and_then(|s| profile::parse_bytes(s));
                        i += 2;
                    }
                    "--json" => i += 1,
                    other if !other.starts_with('-') => {
                        spec.name = other.to_string();
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            match profile::upsert(conn, &spec) {
                Ok(id) => {
                    println!("profile {id} {}", spec.name);
                    0
                }
                Err(e) => fail(&e),
            }
        }
        _ => {
            println!("profile ls|show|set");
            2
        }
    }
}

fn cmd_monitor(conn: &Connection, args: &[String]) -> i32 {
    let json = wants_json(args);
    match args.first().map(|s| s.as_str()).unwrap_or("ls") {
        "ls" => match query::monitors(conn) {
            Ok(rows) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
                    return 0;
                }
                println!("{:<4} {:<20} PROFILE         ENABLED", "ID", "FAMILY");
                for r in rows {
                    println!(
                        "{:<4} {:<20} {:<15} {}",
                        r.id,
                        r.family,
                        r.profile,
                        if r.enabled { "yes" } else { "no" }
                    );
                }
                0
            }
            Err(e) => fail(&e),
        },
        "add" => {
            let mut profile_name = "daily-llm".to_string();
            let mut family = String::new();
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--profile" => {
                        profile_name = args.get(i + 1).cloned().unwrap_or_default();
                        i += 2;
                    }
                    "--json" | "--auto-fetch" => i += 1,
                    other if !other.starts_with('-') && family.is_empty() => {
                        family = other.to_string();
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            if family.is_empty() {
                println!("monitor add FAMILY [--profile daily-llm]");
                return 2;
            }
            let fam = match store::family_find_id(conn, &family) {
                Some(id) => id,
                None => return fail("identity.not_found"),
            };
            if let Err(e) = profile::ensure_default(conn) {
                return fail(&e);
            }
            let pid = match profile::find_id(conn, &profile_name) {
                Some(id) => id,
                None => return fail("identity.not_found"),
            };
            match radar::monitor_add(conn, fam, None, pid, true) {
                Ok(id) => {
                    println!("monitor {id} {family} profile={profile_name}");
                    0
                }
                Err(e) => fail(&e),
            }
        }
        "rm" => {
            let id = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            match radar::monitor_rm(conn, id) {
                Ok(()) => {
                    println!("removed monitor {id}");
                    0
                }
                Err(e) => fail(&e),
            }
        }
        _ => {
            println!("monitor add|ls|rm");
            2
        }
    }
}

fn cmd_radar(conn: &Connection, args: &[String]) -> i32 {
    let json = wants_json(args);
    let mut families = Vec::new();
    let mut once = false;
    for a in args {
        match a.as_str() {
            "--once" => once = true,
            "--json" => {}
            other if !other.starts_with('-') => families.push(other.to_string()),
            _ => {}
        }
    }
    let _ = once;
    println!("{}", radar::PRIVACY_NOTICE);
    let provider = availability::live_hf(true);
    match radar::sweep(conn, &provider, &radar::SweepOpts { opt_in: true, families }) {
        Ok(rep) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&rep).unwrap_or_default());
            } else {
                println!(
                    "sweep monitors={} upserted={} open={} have_bytes={} satisfied={}",
                    rep.monitors, rep.upserted, rep.open, rep.skipped_have_bytes, rep.satisfied
                );
            }
            0
        }
        Err(e) => fail(&e),
    }
}

fn cmd_wanted(conn: &Connection, args: &[String]) -> i32 {
    let json = wants_json(args);
    let mut filter = radar::WantedFilter::default();
    for a in args {
        match a.as_str() {
            "--open" => filter.open = true,
            "--have-offline" => filter.have_offline = true,
            _ => {}
        }
    }
    match query::wanted(conn, &filter) {
        Ok(out) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
                return 0;
            }
            println!("{:<4} {:<18} {:<10} {:<28} {:<20} NOTE", "ID", "FAMILY", "REV", "FILE", "STATUS");
            for w in out.items {
                println!(
                    "{:<4} {:<18} {:<10} {:<28} {:<20} {}",
                    w.id, w.family, w.revision, w.filename, w.status, w.note
                );
            }
            0
        }
        Err(e) => fail(&e),
    }
}

fn cmd_fetch(conn: &Connection, args: &[String]) -> i32 {
    let json = wants_json(args);
    if args.first().map(|s| s.as_str()) == Some("status") {
        match fetch::task_list(conn) {
            Ok(rows) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
                } else {
                    println!("{:<4} {:<10} {:<10} DEST", "ID", "WANTED", "STATE");
                    for r in rows {
                        println!(
                            "{:<4} {:<10} {:<10} {}",
                            r["id"].as_i64().unwrap_or(0),
                            r["wanted_id"].as_i64().unwrap_or(0),
                            r["state"].as_str().unwrap_or(""),
                            r["dest_rel_path"].as_str().unwrap_or("")
                        );
                    }
                }
                0
            }
            Err(e) => fail(&e),
        }
    } else {
        let mut force = false;
        let mut file = None;
        let mut id = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--force" => {
                    force = true;
                    i += 1;
                }
                "--file" => {
                    file = args.get(i + 1).cloned();
                    i += 2;
                }
                "--json" => i += 1,
                other if !other.starts_with('-') && id.is_none() => {
                    id = other.parse().ok();
                    i += 1;
                }
                _ => i += 1,
            }
        }
        let Some(wanted_id) = id else {
            println!("fetch WANTED_ID [--force] [--file NAME] | status");
            return 2;
        };
        match fetch::fetch_wanted(conn, wanted_id, file.as_deref(), force, &fetch::HfHttps) {
            Ok(res) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
                } else {
                    println!("fetch {} {}", res.state, res.dest);
                }
                0
            }
            Err(e) => fail(&e),
        }
    }
}