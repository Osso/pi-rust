//! Native crypto.randomUUID binding used by Codex and coding-agent boundaries.

/// `crypto.randomUUID()`: RFC 9562 version 4 from OS randomness.
pub fn random_uuid() -> String {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).expect("OS random source must be available for UUIDv4");
    bytes[6] = 0x40 | (bytes[6] & 0x0f);
    bytes[8] = 0x80 | (bytes[8] & 0x3f);
    format_uuid(&bytes)
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    let hex: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        hex[0..4].concat(),
        hex[4..6].concat(),
        hex[6..8].concat(),
        hex[8..10].concat(),
        hex[10..16].concat()
    )
}
