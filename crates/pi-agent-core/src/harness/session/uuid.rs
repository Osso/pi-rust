//! Frozen agent-core/session/uuid.ts.

use std::sync::{Mutex, OnceLock};

struct UuidV7Generator {
    last_timestamp: i64,
    sequence: u32,
}

impl UuidV7Generator {
    fn new() -> Self {
        Self {
            last_timestamp: i64::MIN,
            sequence: 0,
        }
    }

    fn generate(&mut self, timestamp: i64, random: [u8; 16]) -> String {
        if timestamp > self.last_timestamp {
            self.sequence = u32::from_be_bytes([random[6], random[7], random[8], random[9]]);
            self.last_timestamp = timestamp;
        } else {
            self.sequence = self.sequence.wrapping_add(1);
            if self.sequence == 0 {
                self.last_timestamp += 1;
            }
        }

        let sequence = self.sequence;
        let timestamp = self.last_timestamp as f64;
        let mut bytes = [0_u8; 16];
        for (index, byte) in bytes.iter_mut().take(6).enumerate() {
            let divisor = (1_u64 << (40 - 8 * index)) as f64;
            *byte = (timestamp / divisor) as i64 as u8;
        }
        bytes[6] = 0x70 | ((sequence >> 28) as u8 & 0x0f);
        bytes[7] = (sequence >> 20) as u8;
        bytes[8] = 0x80 | ((sequence >> 14) as u8 & 0x3f);
        bytes[9] = (sequence >> 6) as u8;
        bytes[10] = (((sequence & 0x3f) << 2) as u8) | (random[10] & 0x03);
        bytes[11..].copy_from_slice(&random[11..]);

        format_uuid(&bytes)
    }
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

static GENERATOR: OnceLock<Mutex<UuidV7Generator>> = OnceLock::new();

/// Generates a time-ordered UUIDv7.
pub fn uuidv7() -> String {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).expect("OS random source must be available for UUIDv7");
    let mut generator = GENERATOR
        .get_or_init(|| Mutex::new(UuidV7Generator::new()))
        .lock()
        .expect("UUID generator mutex poisoned");
    generator.generate(now_millis(), random)
}

fn now_millis() -> i64 {
    pi_js_compat::date::js_now_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::UuidV7Generator;

    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-ai/tests/support/oracle.rs"));
    }

    #[test]
    fn timestamp_sequence_and_random_bit_vectors_match_frozen() {
        let mut oracle = oracle::TsOracle::spawn(
            "agent-core-uuid-oracle.ts",
            "packages/agent-core/src/harness/session/uuid.ts",
        );
        let groups = [
            vec![(-1, [0; 16]), (-2, [0xab; 16]), (0, [0xcd; 16])],
            vec![
                (0x0123_4567_89ab, [0xff; 16]),
                (0x0123_4567_89ab, [0; 16]),
                (0x0123_4567_89aa, [1; 16]),
            ],
            vec![(0x1_0000_0000_0000, [0x55; 16]), (8_640_000_000_000_000, [0xaa; 16])],
        ];
        for (id, steps) in groups.into_iter().enumerate() {
            let mut generator = UuidV7Generator::new();
            let actual = steps
                .iter()
                .map(|(timestamp, random)| generator.generate(*timestamp, *random))
                .collect::<Vec<_>>();
            let request_steps = steps
                .iter()
                .map(|(timestamp, random)| serde_json::json!({"timestamp":timestamp,"random":random}))
                .collect::<Vec<_>>();
            let response = oracle.request(&serde_json::json!({"id":id,"operation":"vectors","steps":request_steps}));
            assert_eq!(response["ok"], true, "{response}");
            assert_eq!(serde_json::json!(actual), response["output"]);
        }
    }

    const TIMESTAMP: i64 = 0x0123_4567_89ab;

    #[test]
    fn matches_the_typescript_rfc9562_layout_and_monotonic_order() {
        let random_values = [
            [
                0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xfe, 0x01, 0x11, 0x22, 0x33, 0x44, 0x55,
            ],
            [0_u8; 16],
            [0_u8; 16],
        ];
        let mut generator = UuidV7Generator::new();

        let first = generator.generate(TIMESTAMP, random_values[0]);
        let second = generator.generate(TIMESTAMP, random_values[1]);
        let third = generator.generate(TIMESTAMP, random_values[2]);

        assert_eq!(first, "01234567-89ab-7fff-bfff-f91122334455");
        assert_eq!(second, "01234567-89ab-7fff-bfff-fc0000000000");
        assert_eq!(third, "01234567-89ac-7000-8000-000000000000");
        assert!(first < second);
        assert!(second < third);
    }

    #[test]
    fn negative_timestamps_follow_frozen_division_then_bitwise_conversion() {
        let mut generator = UuidV7Generator::new();
        assert_eq!(generator.generate(-1, [0; 16]), "00000000-00ff-7000-8000-000000000000");
    }

    #[test]
    fn restarts_the_sequence_when_the_clock_advances() {
        let mut generator = UuidV7Generator::new();
        let first = generator.generate(TIMESTAMP, [0xab; 16]);
        let later = generator.generate(TIMESTAMP + 1, [0xcd; 16]);

        assert!(first.starts_with("01234567-89ab"));
        assert!(later.starts_with("01234567-89ac"));
        assert!(first < later);
        assert_eq!(&later[14..15], "7");
        assert!(matches!(&later[19..20], "8" | "9" | "a" | "b"));
    }
}
