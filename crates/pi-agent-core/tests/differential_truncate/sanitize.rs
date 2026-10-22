use pi_agent_core::harness::utils::sanitize_binary_output;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 11_000;
const FIRST_SOURCE_ONLY_ID: u64 = 11_018;
const LATIN_CAPITAL_A: u16 = 0x0041;
const LATIN_CAPITAL_B: u16 = 0x0042;
const LATIN_CAPITAL_C: u16 = 0x0043;
const LONE_HIGH_SURROGATE: u16 = 0xd800;
const LONE_LOW_SURROGATE: u16 = 0xdc00;
const GRINNING_FACE_HIGH_SURROGATE: u16 = 0xd83d;
const GRINNING_FACE_LOW_SURROGATE: u16 = 0xde00;
const GRINNING_FACE_CODE_POINT: u32 = 0x1f600;

struct Fixture {
    name: &'static str,
    input: String,
}

struct SourceOnlyFixture {
    name: &'static str,
    units: Vec<u16>,
    code_points: Vec<u32>,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    compare_representable_fixtures(oracle);
    compare_source_only_surrogates(oracle);
}

fn compare_representable_fixtures(oracle: &mut TsOracle) {
    for (offset, fixture) in representable_fixtures().into_iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&json!({
            "id": id,
            "operation": "sanitize_binary_output",
            "text": fixture.input,
        }));
        let sanitized = sanitize_binary_output(&fixture.input);
        let actual = ok_response(id, normalize_text(&sanitized));

        assert_eq!(actual, expected, "sanitizer mismatch: {}", fixture.name);
    }
}

fn compare_source_only_surrogates(oracle: &mut TsOracle) {
    for (offset, fixture) in source_only_fixtures().into_iter().enumerate() {
        let id = FIRST_SOURCE_ONLY_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&json!({
            "id": id,
            "operation": "sanitize_utf16_units",
            "units": fixture.units,
        }));
        let actual = ok_response(
            id,
            json!({
                "utf16Units": fixture.units,
                "codePoints": fixture.code_points,
            }),
        );

        assert_eq!(actual, expected, "source-only sanitizer mismatch: {}", fixture.name);
    }
}

fn representable_fixtures() -> Vec<Fixture> {
    vec![
        fixture("empty", ""),
        fixture("ordinary Unicode", "Hello, 世界! 👋"),
        Fixture {
            name: "all C0 controls",
            input: (0_u8..=0x1f).map(char::from).collect(),
        },
        fixture("preserved controls", "\t\n\r"),
        fixture("control mixture", "A\0B\tC\u{1}D\nE\u{2}F\rG"),
        fixture("ASCII and C1 boundaries", "\u{1f} \u{7e}\u{7f}\u{80}\u{85}\u{9f}"),
        fixture("interlinear boundaries", "\u{fff8}\u{fff9}\u{fffa}\u{fffb}\u{fffc}"),
        fixture("interlinear mixture", "a\u{fff9}b\u{fffa}c\u{fffb}d"),
        fixture("supplementary boundaries", "\u{10000}\u{10001}\u{10ffff}"),
        fixture("BMP private use", "\u{e000}\u{f8ff}"),
        fixture("supplementary private use", "\u{f0000}\u{ffffd}\u{100000}\u{10fffd}"),
        fixture("unassigned representatives", "\u{378}\u{379}\u{2fe0}\u{2fef}"),
        fixture("noncharacters", "\u{fdd0}\u{fdef}\u{fffe}\u{ffff}\u{1fffe}\u{10fffe}"),
        fixture("whitespace and separators", " \u{a0}\u{2000}\u{200b}\u{2028}\u{2029}"),
        fixture("combining and variation", "e\u{301}\u{34f}\u{fe0f}"),
        fixture(
            "bidi and format",
            "\u{61c}\u{200e}\u{200f}\u{202a}\u{2060}\u{2066}\u{feff}",
        ),
        fixture("duplicates and order", "x\u{1}x\u{fff9}x\u{7f}x\u{fff9}x"),
        fixture("all removed", "\0\u{1}\u{b}\u{c}\u{1f}\u{fff9}\u{fffa}\u{fffb}"),
    ]
}

fn source_only_fixtures() -> Vec<SourceOnlyFixture> {
    let mut fixtures = lone_surrogate_fixtures();
    fixtures.extend(mixed_surrogate_fixtures());
    fixtures
}

fn lone_surrogate_fixtures() -> Vec<SourceOnlyFixture> {
    vec![
        source_only(
            "lone high surrogate",
            vec![LONE_HIGH_SURROGATE],
            vec![u32::from(LONE_HIGH_SURROGATE)],
        ),
        source_only(
            "lone low surrogate",
            vec![LONE_LOW_SURROGATE],
            vec![u32::from(LONE_LOW_SURROGATE)],
        ),
    ]
}

fn mixed_surrogate_fixtures() -> Vec<SourceOnlyFixture> {
    vec![mixed_lone_surrogate_fixture(), valid_pair_and_lone_units_fixture()]
}

fn mixed_lone_surrogate_fixture() -> SourceOnlyFixture {
    source_only(
        "mixed lone surrogates",
        vec![
            LATIN_CAPITAL_A,
            LONE_HIGH_SURROGATE,
            LATIN_CAPITAL_B,
            LONE_LOW_SURROGATE,
            LATIN_CAPITAL_C,
        ],
        vec![
            u32::from(LATIN_CAPITAL_A),
            u32::from(LONE_HIGH_SURROGATE),
            u32::from(LATIN_CAPITAL_B),
            u32::from(LONE_LOW_SURROGATE),
            u32::from(LATIN_CAPITAL_C),
        ],
    )
}

fn valid_pair_and_lone_units_fixture() -> SourceOnlyFixture {
    source_only(
        "valid pair and lone units",
        vec![
            GRINNING_FACE_HIGH_SURROGATE,
            GRINNING_FACE_LOW_SURROGATE,
            GRINNING_FACE_HIGH_SURROGATE,
            LATIN_CAPITAL_A,
            GRINNING_FACE_LOW_SURROGATE,
        ],
        vec![
            GRINNING_FACE_CODE_POINT,
            u32::from(GRINNING_FACE_HIGH_SURROGATE),
            u32::from(LATIN_CAPITAL_A),
            u32::from(GRINNING_FACE_LOW_SURROGATE),
        ],
    )
}

fn fixture(name: &'static str, input: &str) -> Fixture {
    Fixture {
        name,
        input: input.to_owned(),
    }
}

const fn source_only(name: &'static str, units: Vec<u16>, code_points: Vec<u32>) -> SourceOnlyFixture {
    SourceOnlyFixture {
        name,
        units,
        code_points,
    }
}

fn normalize_text(text: &pi_js_compat::js_json::JsString) -> Value {
    let code_points = char::decode_utf16(text.units().iter().copied())
        .map(|point| {
            point
                .map(u32::from)
                .unwrap_or_else(|error| u32::from(error.unpaired_surrogate()))
        })
        .collect::<Vec<_>>();
    json!({
        "text": text,
        "utf16Units": text.units(),
        "codePoints": code_points,
    })
}
