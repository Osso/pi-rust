//! `Math.random()` for UI effects and retry jitter.

/// Uniform `f64` in `[0, 1)` from the OS random source.
pub fn random_f64() -> f64 {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS random source must be available");
    let value = u64::from_le_bytes(bytes) >> 11;
    value as f64 / (1u64 << 53) as f64
}

/// `Math.floor(Math.random() * length)` for a non-empty slice length.
pub fn random_index(length: usize) -> usize {
    if length == 0 {
        return 0;
    }
    (random_f64() * length as f64).floor() as usize
}
