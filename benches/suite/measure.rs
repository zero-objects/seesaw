//! Measurement of one cell: user CPU time, wall time, peak memory and,
//! with the `perf_trace` feature, the structural counters.

use std::time::Instant;

/// What one measured section cost.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// User CPU time of the process in the section (nanoseconds). Not
    /// affected by other processes taking the cores away.
    pub user_ns: u64,
    /// Wall time of the section (nanoseconds).
    pub wall_ns: u64,
    /// Structural counters (name, value); empty without `perf_trace`.
    pub counters: Vec<(&'static str, u64)>,
}

fn rusage() -> libc::rusage {
    // SAFETY: `getrusage` fills the struct it is handed; a zeroed
    // `rusage` is a valid value of the type.
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        ru
    }
}

fn user_ns() -> u64 {
    let ru = rusage();
    ru.ru_utime.tv_sec as u64 * 1_000_000_000 + ru.ru_utime.tv_usec as u64 * 1_000
}

/// Peak resident set size of the process so far, in bytes.
pub fn peak_rss_bytes() -> u64 {
    let raw = rusage().ru_maxrss as u64;
    // macOS reports bytes, Linux kilobytes.
    if cfg!(target_os = "macos") {
        raw
    } else {
        raw * 1024
    }
}

/// Runs `f` as the measured section of a cell.
pub fn section<T>(f: impl FnOnce() -> T) -> (T, Usage) {
    #[cfg(feature = "perf_trace")]
    seesaw_tgg::perf_trace::reset();
    let wall = Instant::now();
    let user = user_ns();
    let out = f();
    let user_ns = user_ns() - user;
    let wall_ns = wall.elapsed().as_nanos() as u64;
    #[cfg(feature = "perf_trace")]
    let counters = seesaw_tgg::perf_trace::snapshot().fields().to_vec();
    #[cfg(not(feature = "perf_trace"))]
    let counters = Vec::new();
    (
        out,
        Usage {
            user_ns,
            wall_ns,
            counters,
        },
    )
}
