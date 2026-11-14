//! Measurement of one cell: user CPU time, wall time, peak memory,
//! hardware counters and, with the `perf_trace` feature, the structural
//! counters.

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
    /// The hardware events of [`HARDWARE`] in the section, in user mode;
    /// 0 where a counter is not available or did not run for the whole
    /// section. They depend on the machine and, less than time, on what
    /// else runs on it.
    pub hardware: [u64; HARDWARE.len()],
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

/// Hardware events counted besides the instructions: name of the
/// column, `perf_event_attr.type` and `perf_event_attr.config`.
///
/// - `cycles`: CPU cycles.
/// - `llc_miss`: misses of the last-level cache (memory accesses that
///   went to main memory).
/// - `l1d_miss`: read misses of the first-level data cache.
/// - `br_miss`: mispredicted branches.
pub const HARDWARE: [(&str, u32, u64); 4] = [
    ("cycles", 0, 0),
    ("llc_miss", 0, 3),
    ("l1d_miss", 3, 0x1_0000),
    ("br_miss", 0, 5),
];

/// `perf_event_attr.type` and `.config` of the retired instructions.
#[cfg(target_os = "linux")]
const INSTRUCTIONS: (u32, u64) = (0, 1);

/// Counter of one hardware event of this thread in user mode (Linux
/// `perf_event_open`).
#[cfg(target_os = "linux")]
struct Counter(libc::c_int);

/// One reading of a [`Counter`]: the count, and the times the counter
/// was enabled and actually counting. The two differ when the kernel
/// shared the hardware counter with other events.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
struct Reading {
    value: u64,
    enabled: u64,
    running: u64,
}

#[cfg(target_os = "linux")]
impl Counter {
    fn open((kind, config): (u32, u64)) -> Option<Self> {
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
        const TOTAL_TIME_ENABLED: u64 = 1 << 0;
        const TOTAL_TIME_RUNNING: u64 = 1 << 1;
        const EXCLUDE_KERNEL: u64 = 1 << 5;
        const EXCLUDE_HV: u64 = 1 << 6;
        let attr = Attr {
            kind,
            size: std::mem::size_of::<Attr>() as u32,
            config,
            sample_period: 0,
            sample_type: 0,
            read_format: TOTAL_TIME_ENABLED | TOTAL_TIME_RUNNING,
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

    fn read(&self) -> Reading {
        let mut reading = Reading::default();
        let size = std::mem::size_of::<Reading>();
        // SAFETY: reads at most `size` bytes into `reading`, a `repr(C)`
        // struct of three `u64` for which every bit pattern is valid.
        let n = unsafe { libc::read(self.0, (&mut reading as *mut Reading).cast(), size) };
        if n == size as isize {
            reading
        } else {
            Reading::default()
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for Counter {
    fn drop(&mut self) {
        // SAFETY: the descriptor is owned by this value.
        unsafe {
            libc::close(self.0);
        }
    }
}

/// A counter with its reading at the start of the section.
#[cfg(target_os = "linux")]
struct Running(Option<(Counter, Reading)>);

#[cfg(target_os = "linux")]
impl Running {
    fn start(event: (u32, u64)) -> Self {
        Self(Counter::open(event).map(|c| {
            let at = c.read();
            (c, at)
        }))
    }

    /// The count since `start`; 0 without a counter or when it did not
    /// count for the whole time.
    fn stop(self) -> u64 {
        let Some((counter, from)) = self.0 else {
            return 0;
        };
        let to = counter.read();
        if to.enabled - from.enabled == to.running - from.running {
            to.value - from.value
        } else {
            0
        }
    }
}

/// Runs `f` as the measured section of a cell.
pub fn section<T>(f: impl FnOnce() -> T) -> (T, Usage) {
    #[cfg(feature = "perf_trace")]
    seesaw_tgg::perf_trace::reset();
    let wall = Instant::now();
    let user = user_ns();
    #[cfg(target_os = "linux")]
    let (hardware, instructions) = (
        HARDWARE.map(|(_, kind, config)| Running::start((kind, config))),
        Running::start(INSTRUCTIONS),
    );
    let out = f();
    #[cfg(target_os = "linux")]
    let (instructions, hardware) = (instructions.stop(), hardware.map(Running::stop));
    #[cfg(not(target_os = "linux"))]
    let (instructions, hardware) = (0, [0; HARDWARE.len()]);
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
            hardware,
            counters,
        },
    )
}
