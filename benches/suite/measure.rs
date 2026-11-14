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
    /// Instructions retired in user mode in the section; 0 where the
    /// counter is not available (not Linux, or not permitted). Unlike
    /// time it does not depend on what else runs on the machine.
    pub instructions: u64,
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

/// Counter of the instructions this thread retires in user mode
/// (Linux `perf_event_open`).
#[cfg(target_os = "linux")]
struct Instructions(libc::c_int);

#[cfg(target_os = "linux")]
impl Instructions {
    fn open() -> Option<Self> {
        // `perf_event_attr` in its first published layout (64 bytes).
        #[repr(C)]
        struct Attr {
            kind: u32,
            size: u32,
            config: u64,
            sample_period: u64,
            sample_type: u64,
            read_format: u64,
            flags: u64,
            wakeup_events: u32,
            bp_type: u32,
            config1: u64,
        }
        const HARDWARE: u32 = 0;
        const INSTRUCTIONS: u64 = 1;
        const EXCLUDE_KERNEL: u64 = 1 << 5;
        const EXCLUDE_HV: u64 = 1 << 6;
        let attr = Attr {
            kind: HARDWARE,
            size: std::mem::size_of::<Attr>() as u32,
            config: INSTRUCTIONS,
            sample_period: 0,
            sample_type: 0,
            read_format: 0,
            flags: EXCLUDE_KERNEL | EXCLUDE_HV,
            wakeup_events: 0,
            bp_type: 0,
            config1: 0,
        };
        // SAFETY: the kernel reads `size` bytes of `attr`, which is a
        // live value of that size; the remaining arguments select this
        // thread on any CPU without a group.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                &attr as *const Attr,
                0 as libc::pid_t,
                -1 as libc::c_int,
                -1 as libc::c_int,
                0 as libc::c_ulong,
            )
        };
        (fd >= 0).then_some(Self(fd as libc::c_int))
    }

    fn read(&self) -> u64 {
        let mut value = 0u64;
        // SAFETY: reads at most eight bytes into `value`.
        let n = unsafe { libc::read(self.0, (&mut value as *mut u64).cast(), 8) };
        if n == 8 {
            value
        } else {
            0
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for Instructions {
    fn drop(&mut self) {
        // SAFETY: the descriptor is owned by this value.
        unsafe {
            libc::close(self.0);
        }
    }
}

/// Runs `f` as the measured section of a cell.
pub fn section<T>(f: impl FnOnce() -> T) -> (T, Usage) {
    #[cfg(feature = "perf_trace")]
    seesaw_tgg::perf_trace::reset();
    #[cfg(target_os = "linux")]
    let counter = Instructions::open();
    let wall = Instant::now();
    let user = user_ns();
    #[cfg(target_os = "linux")]
    let before = counter.as_ref().map_or(0, Instructions::read);
    let out = f();
    #[cfg(target_os = "linux")]
    let instructions = counter.as_ref().map_or(0, Instructions::read) - before;
    #[cfg(not(target_os = "linux"))]
    let instructions = 0;
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
            instructions,
            counters,
        },
    )
}
