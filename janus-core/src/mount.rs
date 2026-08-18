use std::path::Path;

pub const MARKER: &str = ".janus-root";

/// Real volume UUID / serial only. Never invent a fake id from the path.
pub fn detect_mount_id(path: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        win_volume_serial(path)
    }
    #[cfg(unix)]
    {
        linux_fs_uuid(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
}

/// Use a detected volume id, an existing `.janus-root`, or write one if opted in.
pub fn resolve_mount_id(path: &Path, detected: Option<String>, accept_marker: bool) -> Result<String, String> {
    if let Some(id) = detected.filter(|s| !s.is_empty()) {
        return Ok(id);
    }
    let marker = path.join(MARKER);
    if let Ok(existing) = std::fs::read_to_string(&marker) {
        let id = existing.trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }
    if !accept_marker {
        return Err("root.no_mount_id".into());
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let id = blake3::hash(&nanos.to_le_bytes()).to_hex().to_string();
    std::fs::write(&marker, format!("{id}\n")).map_err(|e| format!("root.no_mount_id: {e}"))?;
    Ok(id)
}

#[cfg(windows)]
fn win_volume_serial(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut vol = [0u16; 512];
    let ok = unsafe { GetVolumePathNameW(wide.as_ptr(), vol.as_mut_ptr(), vol.len() as u32) };
    if ok == 0 {
        return None;
    }
    let mut serial = 0u32;
    let ok = unsafe {
        GetVolumeInformationW(
            vol.as_ptr(),
            std::ptr::null_mut(),
            0,
            &mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return None;
    }
    Some(format!("{serial:08X}"))
}

#[cfg(windows)]
extern "system" {
    fn GetVolumePathNameW(file: *const u16, volume: *mut u16, len: u32) -> i32;
    fn GetVolumeInformationW(
        root: *const u16,
        name: *mut u16,
        name_len: u32,
        serial: *mut u32,
        max_comp: *mut u32,
        flags: *mut u32,
        fs: *mut u16,
        fs_len: u32,
    ) -> i32;
}

#[cfg(unix)]
fn linux_fs_uuid(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    let dev = meta.dev();
    let dir = Path::new("/dev/disk/by-uuid");
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let target = std::fs::canonicalize(e.path()).ok()?;
        let tmeta = std::fs::metadata(&target).ok()?;
        if tmeta.rdev() == dev {
            return Some(e.file_name().to_string_lossy().into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_id_is_real_or_absent() {
        let p = Path::new("/definitely/not/a/real/janus-mount-test-path");
        match detect_mount_id(p) {
            None => {}
            Some(id) => {
                assert!(
                    id.chars().all(|c| c.is_ascii_hexdigit()) && id.len() >= 4,
                    "must be a real volume serial, not a path hash: {id}"
                );
            }
        }
    }

    #[test]
    fn missing_uuid_without_marker_is_refused() {
        let dir = std::env::temp_dir().join(format!("janus-nomount-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = resolve_mount_id(&dir, None, false).unwrap_err();
        assert_eq!(err, "root.no_mount_id");
        assert!(!dir.join(MARKER).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accept_marker_writes_janus_root() {
        let dir = std::env::temp_dir().join(format!("janus-marker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = resolve_mount_id(&dir, None, true).unwrap();
        assert!(!id.is_empty());
        assert_eq!(std::fs::read_to_string(dir.join(MARKER)).unwrap().trim(), id);
        let again = resolve_mount_id(&dir, None, false).unwrap();
        assert_eq!(again, id);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
