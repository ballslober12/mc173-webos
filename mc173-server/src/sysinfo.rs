//! Small helpers to read host system info (uname, memory). Shared by the in-game
//! `/info` command (`command.rs`) and the server console's `info` command
//! (`console.rs`/`server.rs`), so the two report the exact same thing.

/// Run "uname -a" and return its trimmed output. "uname -a" is present on virtually
/// any Linux, including the BusyBox userland webOS ships, and gives the single most
/// useful "what am I actually running on" summary (kernel name, hostname, kernel
/// release/version, machine arch) without needing to hand-read half a dozen /proc
/// files. Falls back to what Rust itself knows about the target this binary was built
/// for if "uname" is missing or fails (e.g. a non-Linux target), so callers always get
/// something useful instead of nothing.
pub fn uname() -> String {
    match std::process::Command::new("uname").arg("-a").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => format!("unavailable (built for {}-{})", std::env::consts::OS, std::env::consts::ARCH),
    }
}

/// Read total and available memory, in kB, from /proc/meminfo. Returns `None` if the
/// file can't be read or doesn't contain the expected fields (e.g. running on a
/// non-Linux target) - this is diagnostic info only, so callers should degrade
/// gracefully rather than treating this as an error.
pub fn meminfo() -> Option<(u64, u64)> {

    let content = std::fs::read_to_string("/proc/meminfo").ok()?;

    let mut total = None;
    let mut available = None;
    let mut free = None;

    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else { continue };
        let Some(value_kb) = parts.next().and_then(|v| v.parse::<u64>().ok()) else { continue };
        match key {
            "MemTotal:" => total = Some(value_kb),
            "MemAvailable:" => available = Some(value_kb),
            "MemFree:" => free = Some(value_kb),
            _ => {}
        }
    }

    // Prefer MemAvailable (accounts for reclaimable page cache, so it reflects what's
    // actually usable) where the kernel exposes it; older/minimal kernels (plausible
    // on embedded webOS) may only have MemFree, which undercounts real availability
    // but is still far better than nothing.
    Some((total?, available.or(free)?))

}
