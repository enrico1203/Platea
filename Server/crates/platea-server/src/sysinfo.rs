//! Misure della macchina per la dashboard (server e nodi): memoria, carico, spazio libero.

/// Memoria residente del processo, in byte.
pub fn rss() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// (memoria totale, memoria disponibile) della macchina, in byte.
pub fn meminfo() -> (Option<u64>, Option<u64>) {
    let Ok(s) = std::fs::read_to_string("/proc/meminfo") else { return (None, None) };
    let field = |name: &str| -> Option<u64> {
        let line = s.lines().find(|l| l.starts_with(name))?;
        line.split_whitespace().nth(1)?.parse::<u64>().ok().map(|kb| kb * 1024)
    };
    (field("MemTotal:"), field("MemAvailable:"))
}

/// Carico medio dell'ultimo minuto.
pub fn load_1m() -> Option<f64> {
    std::fs::read_to_string("/proc/loadavg").ok().and_then(|l| l.split_whitespace().next().and_then(|x| x.parse::<f64>().ok()))
}

/// Dimensione totale del filesystem di `path`, in byte.
pub fn disk_bytes(path: &std::path::Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0 {
            return Some(st.f_blocks as u64 * st.f_frsize as u64);
        }
        None
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// Spazio libero per i non amministratori sul filesystem di `path`, in byte.
pub fn free_bytes(path: &std::path::Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0 {
            return Some(st.f_bavail as u64 * st.f_frsize as u64);
        }
        None
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}
