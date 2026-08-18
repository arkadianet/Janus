use janus_core::{cases, db, dedup, doctor, export, identity, parse, scan, store};
use rusqlite::{Connection, params};
use std::io::BufRead;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        return;
    }
    let conn = open_db();
    let code = match args[1].as_str() {
        "db" => cmd_db(&conn, &args[2..]),
        "root" => cmd_root(&conn, &args[2..]),
        "scan" => cmd_scan(&conn, &args[2..]),
        "status" => cmd_status(&conn),
        "list" => cmd_list(&conn),
        "show" => cmd_show(&conn, &args[2..]),
        "identify" => cmd_identify(&args[2..]),
        "dedup" => cmd_dedup(&conn),
        "doctor" => cmd_doctor(&conn, &args[2..]),
        "decline" => cmd_decline(&conn, &args[2..]),
        "export" => cmd_export(&conn, &args[2..]),
        "cases" => cmd_cases(&args[2..]),
        "have" => cmd_have(&conn, &args[2..]),
        _ => {
            usage();
            2
        }
    };
    std::process::exit(code);
}

fn usage() {
    println!(
        "janus <cmd>\n  db\n  root add|ls|rm\n  scan [--quick] ROOT\n  status\n  list\n  show FAMILY\n  identify FILE\n  dedup\n  doctor\n  decline KEY_A KEY_B\n  export PATH\n  cases [FIXTURES_DIR]\n  have rel_path --root ID"
    );
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
    if args.is_empty() {
        for r in store::root_ls(conn).unwrap_or_default() {
            println!("{} {} {} present={} last_scan={:?}", r.id, r.kind, r.path.as_str(), r.present.unwrap_or(0) == 1, r.last_scan_at);
        }
        return 0;
    }
    match args[0].as_str() {
        "add" => {
            let mut kind = "internal".to_string();
            let mut name: Option<String> = None;
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
                    _ => break,
                }
            }
            let path = args.get(i).cloned().unwrap_or_default();
            if path.is_empty() {
                println!("root add: missing PATH");
                return 2;
            }
            let name = name.unwrap_or_else(|| Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(path.clone()));
            match store::root_add(conn, &name, &path, &kind) {
                Ok(id) => {
                    println!("added root {id} {name} {path} kind={kind}");
                    0
                }
                Err(e) => {
                    println!("error: {e}");
                    1
                }
            }
        }
        "ls" => cmd_root(conn, &[]),
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
            if store::root_by_id(conn, id).is_err() {
                println!("error: no such root {id}");
                return 1;
            }
            let tx = match conn.unchecked_transaction() {
                Ok(t) => t,
                Err(e) => {
                    println!("error: {e}");
                    return 1;
                }
            };
            if let Err(e) = tx.execute(
                "DELETE FROM file_roles WHERE file_id IN (SELECT id FROM files WHERE root_id=?1)",
                [id],
            ) {
                println!("error: {e}");
                return 1;
            }
            if let Err(e) = tx.execute("DELETE FROM files WHERE root_id=?1", [id]) {
                println!("error: {e}");
                return 1;
            }
            match tx.execute("DELETE FROM storage_roots WHERE id=?1", [id]) {
                Ok(0) => {
                    println!("error: no such root {id}");
                    1
                }
                Ok(_) => match tx.commit() {
                    Ok(()) => {
                        println!("removed root {id}");
                        0
                    }
                    Err(e) => {
                        println!("error: {e}");
                        1
                    }
                },
                Err(e) => {
                    println!("error: {e}");
                    1
                }
            }
        }
        _ => {
            usage();
            2
        }
    }
}

fn cmd_scan(conn: &Connection, args: &[String]) -> i32 {
    let quick = args.iter().any(|a| a == "--quick");
    let target = args.iter().find(|a| !a.starts_with('-')).cloned().unwrap_or_default();
    let Some(root_id) = resolve_root(conn, &target) else {
        println!("scan: unknown root {target}");
        return 2;
    };
    let opts = scan::ScanOptions { quick };
    match scan::scan_root(conn, root_id, &opts) {
        Ok(rep) => {
            let mode = if quick { "quick" } else { "full" };
            println!(
                "{mode} scan: seen={} new={} changed={} unsupported={} unverified={} families_new={} symlink_dirs_skipped={} dirs_unreadable={} non_utf8={} deep={} root_offline={}",
                rep.files_seen, rep.files_new, rep.files_changed, rep.files_unsupported, rep.files_unverified, rep.families_new, rep.skipped_symlink_dirs, rep.dirs_unreadable, rep.skipped_non_utf8, rep.skipped_deep, rep.root_offline
            );
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_status(conn: &Connection) -> i32 {
    match store::home_counts(conn) {
        Ok((families, families_inferred, bytes, unverified, unknown_files)) => {
            println!("families: {families} ({families_inferred} name-inferred)");
            println!("bytes indexed: {bytes}");
            println!("files not full-hashed: {unverified}");
            println!("filenames with content role unattached: {unknown_files}");
            let (all, present) = store::present_count(conn).unwrap_or((0, 0));
            println!("roots: {present}/{all} present");
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn cmd_list(conn: &Connection) -> i32 {
    use janus_core::identity::round1;
    let fams = match store::family_list(conn) {
        Ok(f) => f,
        Err(e) => {
            println!("error: {e}");
            return 1;
        }
    };
    if fams.is_empty() {
        println!("no families (run: janus root add PATH && janus scan)");
        return 0;
    }
    println!("{:<28} {:<5} {:<9} {:<22} {:<10} ROOTS", "FAMILY", "KIND", "PARAMS", "VARIANTS", "SIZE");
    for f in &fams {
        let name = f.name.as_deref().unwrap_or(f.key.split('|').next().unwrap_or("unknown"));
        let params = match f.params_total {
            Some(t) => format!("{:>5.1}B", t),
            None => "—".to_string(),
        };
        let roots: Vec<String> = f
            .roots
            .iter()
            .map(|(n, present)| if *present { n.clone() } else { format!("[{n}]") })
            .collect();
        let roots = roots.join(",");
        let roots = if roots.is_empty() { "—".to_string() } else { roots };
        println!(
            "{:<28} {:<5} {:<9} {:<22} {:<10} {}",
            name,
            f.kind,
            params,
            short_quants(&f.quants),
            human_bytes(f.bytes),
            roots
        );
    }
    let _ = round1;
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

fn cmd_identify(args: &[String]) -> i32 {
    let non_interactive = args.iter().any(|a| a == "--non-interactive");
    let path = args.iter().find(|a| !a.starts_with('-')).map(String::as_str).unwrap_or_default();
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
    if is_unknown && !non_interactive {
        println!("---");
        println!("Name this file? (empty to skip)");
        let line = std::io::stdin().lock().lines().next().and_then(|l| l.ok()).unwrap_or_default();
        let name = line.trim().to_string();
        if !name.is_empty() {
            println!("(would record name '{name}' as manual — not yet wired to a root)");
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

fn cmd_dedup(conn: &Connection) -> i32 {
    let groups = dedup::plan(conn);
    let mut files: i64 = 0;
    let mut bytes: i64 = 0;
    for g in &groups {
        files += g.reclaimable_files;
        bytes += g.reclaimable_bytes;
    }
    println!("duplicate groups: {}", groups.len());
    println!("reclaimable files: {files}  reclaimable bytes: {bytes}");
    for g in &groups {
        println!("{}  size={} allocations={} reclaimable={} copies={}", g.blake3, g.size, g.allocations, g.reclaimable_files, g.copies.len());
        for c in &g.copies {
            println!("    {} {} ino={}", c.root_name, c.rel_path, c.ino);
        }
    }
    0
}

fn cmd_doctor(conn: &Connection, _args: &[String]) -> i32 {
    let suggestions = doctor::sweep(conn);
    if suggestions.is_empty() {
        println!("no merge suggestions");
    }
    for s in &suggestions {
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