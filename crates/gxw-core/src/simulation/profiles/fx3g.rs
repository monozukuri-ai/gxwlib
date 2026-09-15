//! Fixed, explicitly selected FX3G defaults, not decoded GXW parameters.
pub const TIMING_MODEL: &str = "scan_start_coil_clock_v1";
pub const RETENTION_POLICY: &str = "fx3g_default_m8033_off";
pub const MAX_PHASE_NS: u64 = 100_000_000;

/// Coil-evaluated timers only. Routine/interrupt timers need another schedule.
pub fn timer(address: u32) -> Option<(u64, bool)> {
    match address {
        0..=191 => Some((100_000_000, false)),
        200..=245 => Some((10_000_000, false)),
        250..=255 => Some((100_000_000, true)),
        256..=319 => Some((1_000_000, false)),
        _ => None,
    }
}
pub fn counter_retentive(address: u32) -> bool {
    (16..=199).contains(&address)
}
pub fn relay_retentive(address: u32) -> bool {
    (384..=1535).contains(&address)
}

// A common virtual oscillator offset, reduced independently for each base.
// u128 avoids wrapping when an otherwise valid u64 time is near its limit.
pub fn ticks(time_ns: u64, phase_ns: u64, base_ns: u64) -> u64 {
    ((u128::from(time_ns) + u128::from(phase_ns)) / u128::from(base_ns)) as u64
}
pub fn phase(time_ns: u64, phase_ns: u64, base_ns: u64) -> u64 {
    ((u128::from(time_ns) + u128::from(phase_ns)) % u128::from(base_ns)) as u64
}
