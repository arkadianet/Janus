use std::path::Path;

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
}
