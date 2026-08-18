use janus_core::{cases, db, dedup, doctor, export, scan, store};
use rusqlite::{Connection, params};
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
        "janus <cmd>\n  db\n  root add|ls|rm\n  scan [--quick] ROOT\n  status\n  dedup\n  doctor\n  decline KEY_A KEY_B\n  export PATH\n  cases [FIXTURES_DIR]\n  have rel_path --root ID"
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
            let id: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(-1);
            match conn.execute("DELETE FROM files WHERE root_id=?1", [id]).and_then(|_| conn.execute("DELETE FROM storage_roots WHERE id=?1", [id])) {
                Ok(_) => {
                    println!("removed root {id}");
                    0
                }
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
                "{mode} scan: seen={} new={} changed={} unsupported={} unverified={} families_new={} symlink_dirs_skipped={} root_offline={}",
                rep.files_seen, rep.files_new, rep.files_changed, rep.files_unsupported, rep.files_unverified, rep.families_new, rep.skipped_symlink_dirs, rep.root_offline
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
    let rel = args.iter().find(|a| !a.starts_with('-')).cloned().unwrap_or_default();
    let mut root_id = 0i64;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--root" {
            root_id = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
            break;
        }
        i += 1;
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