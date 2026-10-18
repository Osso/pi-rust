use super::oracle::TsOracle;
use pi_protocol::{CborCodecError, CborOptions, CborValue, decode_cbor, encode_cbor};
use serde_json::{Value, json};

enum Operation {
    Encode {
        value: CborValue,
        options: Option<CborOptions>,
    },
    Decode {
        bytes: Vec<u8>,
        options: Option<CborOptions>,
    },
}

struct Fixture {
    name: String,
    operation: Operation,
}

impl Fixture {
    fn request(&self, id: u64) -> Value {
        match &self.operation {
            Operation::Encode { value, options } => {
                let mut request = json!({
                    "id": id,
                    "operation": "encode_cbor",
                    "value": tagged(value),
                });
                add_options(&mut request, *options);
                request
            }
            Operation::Decode { bytes, options } => {
                let mut request = json!({
                    "id": id,
                    "operation": "decode_cbor",
                    "encoded_hex": hex_encode(bytes),
                });
                add_options(&mut request, *options);
                request
            }
        }
    }

    fn rust_response(&self, id: u64) -> Value {
        match &self.operation {
            Operation::Encode { value, options } => match encode_cbor(value, *options) {
                Ok(encoded) => json!({
                    "id": id,
                    "ok": true,
                    "output": {"encoded_hex": hex_encode(&encoded)},
                }),
                Err(error) => error_response(id, &error),
            },
            Operation::Decode { bytes, options } => match decode_cbor(bytes, *options) {
                Ok(value) => json!({
                    "id": id,
                    "ok": true,
                    "output": {"value": tagged(&value)},
                }),
                Err(error) => error_response(id, &error),
            },
        }
    }
}

fn add_options(request: &mut Value, options: Option<CborOptions>) {
    let Some(options) = options else {
        return;
    };
    let mut encoded = serde_json::Map::new();
    if let Some(value) = options.max_byte_length {
        encoded.insert("maxByteLength".to_string(), json!(value));
    }
    if let Some(value) = options.max_container_length {
        encoded.insert("maxContainerLength".to_string(), json!(value));
    }
    if let Some(value) = options.max_depth {
        encoded.insert("maxDepth".to_string(), json!(value));
    }
    request["options"] = Value::Object(encoded);
}

fn error_response(id: u64, error: &CborCodecError) -> Value {
    let name = match error {
        CborCodecError::Cbor(_) => "CborError",
        CborCodecError::Options(_) => "RangeError",
    };
    json!({
        "id": id,
        "ok": false,
        "error": {"name": name, "message": error.to_string()},
    })
}

pub(crate) fn tagged(value: &CborValue) -> Value {
    match value {
        CborValue::Null => json!({"t": "null"}),
        CborValue::Bool(value) => json!({"t": "bool", "v": value}),
        CborValue::Number(value) => json!({"t": "number", "bits": format!("{:016x}", value.to_bits())}),
        CborValue::Text(value) => json!({"t": "string", "v": value}),
        CborValue::Bytes(value) => json!({"t": "bytes", "hex": hex_encode(value)}),
        CborValue::Array(values) => json!({
            "t": "array",
            "v": values.iter().map(tagged).collect::<Vec<_>>(),
        }),
        CborValue::Map(entries) => json!({
            "t": "object",
            "v": entries
                .iter()
                .map(|(key, value)| json!([key, tagged(value)]))
                .collect::<Vec<_>>(),
        }),
    }
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
}

fn from_hex(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("hex must be valid"))
        .collect()
}

fn text(value: &str) -> CborValue {
    CborValue::Text(value.to_string())
}

fn number(value: f64) -> CborValue {
    CborValue::Number(value)
}

fn array(values: Vec<CborValue>) -> CborValue {
    CborValue::Array(values)
}

fn map(entries: Vec<(&str, CborValue)>) -> CborValue {
    CborValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn encode_fixture(name: impl Into<String>, value: CborValue, options: Option<CborOptions>) -> Fixture {
    Fixture {
        name: name.into(),
        operation: Operation::Encode { value, options },
    }
}

fn decode_fixture(name: impl Into<String>, bytes: Vec<u8>, options: Option<CborOptions>) -> Fixture {
    Fixture {
        name: name.into(),
        operation: Operation::Decode { bytes, options },
    }
}

fn encode_vectors() -> Vec<Fixture> {
    let vectors = vec![
        (CborValue::Null, "f6"),
        (CborValue::Bool(false), "f4"),
        (CborValue::Bool(true), "f5"),
        (number(0.0), "00"),
        (number(1.0), "01"),
        (number(10.0), "0a"),
        (number(23.0), "17"),
        (number(24.0), "1818"),
        (number(25.0), "1819"),
        (number(100.0), "1864"),
        (number(1000.0), "1903e8"),
        (number(1_000_000.0), "1a000f4240"),
        (number(1_000_000_000_000.0), "1b000000e8d4a51000"),
        (number(9_007_199_254_740_991.0), "1b001fffffffffffff"),
        (number(-1.0), "20"),
        (number(-10.0), "29"),
        (number(-24.0), "37"),
        (number(-25.0), "3818"),
        (number(-100.0), "3863"),
        (number(-1000.0), "3903e7"),
        (number(-1_000_000.0), "3a000f423f"),
        (number(-9_007_199_254_740_991.0), "3b001ffffffffffffe"),
        (number(1.1), "fb3ff199999999999a"),
        (number(-0.0), "fb8000000000000000"),
        (CborValue::Bytes(vec![1, 2, 3, 4]), "4401020304"),
        (CborValue::Bytes(Vec::new()), "40"),
        (text(""), "60"),
        (text("IETF"), "6449455446"),
        (text("ü"), "62c3bc"),
        (text("水"), "63e6b0b4"),
        (text("𐅑"), "64f0908591"),
        (text("\u{feff}"), "63efbbbf"),
        (array(Vec::new()), "80"),
        (array(vec![number(1.0), number(2.0), number(3.0)]), "83010203"),
        (
            array(vec![
                number(1.0),
                array(vec![number(2.0), number(3.0)]),
                array(vec![number(4.0), number(5.0)]),
            ]),
            "8301820203820405",
        ),
        (
            map(vec![("a", number(1.0)), ("b", array(vec![number(2.0), number(3.0)]))]),
            "a26161016162820203",
        ),
        (
            map(vec![
                ("zero", number(0.0)),
                ("empty", text("")),
                ("no", CborValue::Bool(false)),
                ("nil", CborValue::Null),
            ]),
            "a4647a65726f0065656d70747960626e6ff4626e696cf6",
        ),
        (
            map(vec![
                ("__proto__", text("safe")),
                ("nested", map(vec![("k", text("v"))])),
            ]),
            "a2695f5f70726f746f5f5f6473616665666e6573746564a1616b6176",
        ),
    ];

    vectors
        .into_iter()
        .map(|(value, wire)| encode_fixture(format!("encode {wire}"), value, None))
        .collect()
}

fn encode_error_fixtures() -> Vec<Fixture> {
    let container = CborOptions {
        max_container_length: Some(2),
        ..CborOptions::default()
    };
    let bytes = CborOptions {
        max_byte_length: Some(2),
        ..CborOptions::default()
    };
    let depth = CborOptions {
        max_depth: Some(513),
        ..CborOptions::default()
    };
    let zero_depth = CborOptions {
        max_depth: Some(0),
        ..CborOptions::default()
    };

    let mut too_deep = CborValue::Null;
    for _ in 0..=64 {
        too_deep = array(vec![too_deep]);
    }

    vec![
        encode_fixture("encode NaN", number(f64::NAN), None),
        encode_fixture("encode positive infinity", number(f64::INFINITY), None),
        encode_fixture("encode negative infinity", number(f64::NEG_INFINITY), None),
        encode_fixture("encode unsafe positive integer", number(9_007_199_254_740_992.0), None),
        encode_fixture("encode unsafe negative integer", number(-9_007_199_254_740_992.0), None),
        encode_fixture("encode excessive depth", too_deep, None),
        encode_fixture(
            "encode container limit",
            array(vec![number(1.0), number(2.0), number(3.0)]),
            Some(container),
        ),
        encode_fixture("encode text limit", text("ab"), Some(bytes)),
        encode_fixture("encode invalid depth option", number(1.0), Some(depth)),
        encode_fixture("encode depth zero", array(vec![number(1.0)]), Some(zero_depth)),
    ]
}

fn decode_vectors() -> Vec<Fixture> {
    let wires = [
        "f6",
        "f4",
        "f5",
        "00",
        "01",
        "0a",
        "17",
        "1818",
        "1819",
        "1864",
        "1903e8",
        "1a000f4240",
        "1b000000e8d4a51000",
        "1b001fffffffffffff",
        "20",
        "29",
        "37",
        "3818",
        "3863",
        "3903e7",
        "3a000f423f",
        "3b001ffffffffffffe",
        "fb3ff199999999999a",
        "fb8000000000000000",
        "4401020304",
        "40",
        "60",
        "6449455446",
        "62c3bc",
        "63e6b0b4",
        "64f0908591",
        "63efbbbf",
        "80",
        "83010203",
        "8301820203820405",
        "a26161016162820203",
    ];
    wires
        .into_iter()
        .map(|wire| decode_fixture(format!("decode {wire}"), from_hex(wire), None))
        .collect()
}

fn decode_error_fixtures() -> Vec<Fixture> {
    let wires = [
        ("empty input", ""),
        ("truncated integer", "18"),
        ("reserved additional information", "1c"),
        ("indefinite byte string", "5f"),
        ("indefinite text string", "7f"),
        ("indefinite array", "9f"),
        ("indefinite map", "bf"),
        ("tag", "c000"),
        ("undefined", "f7"),
        ("unsupported simple value", "e0"),
        ("break outside an indefinite item", "ff"),
        ("float16", "f93c00"),
        ("float32", "fa3f800000"),
        ("positive infinity", "fb7ff0000000000000"),
        ("NaN", "fb7ff8000000000000"),
        ("truncated float64", "fb3ff00000"),
        ("truncated byte string", "44010203"),
        ("truncated text string", "636162"),
        ("truncated array", "8201"),
        ("truncated map", "a16161"),
        ("trailing data", "0000"),
        ("non-string map key", "a10102"),
        ("duplicate map key", "a2616101616102"),
        ("invalid UTF-8 byte", "61ff"),
        ("overlong UTF-8", "62c080"),
        ("UTF-8 surrogate", "63eda080"),
        ("unsafe positive integer", "1b0020000000000000"),
        ("unsafe negative integer", "3b001fffffffffffff"),
        ("unsafe integer encoded as float64", "fb4340000000000000"),
    ];
    wires
        .into_iter()
        .map(|(label, wire)| decode_fixture(format!("decode {label}"), from_hex(wire), None))
        .collect()
}

fn limit_fixtures() -> Vec<Fixture> {
    let container = CborOptions {
        max_container_length: Some(2),
        ..CborOptions::default()
    };
    let bytes = CborOptions {
        max_byte_length: Some(2),
        ..CborOptions::default()
    };
    let depth = CborOptions {
        max_depth: Some(513),
        ..CborOptions::default()
    };
    let zero_depth = CborOptions {
        max_depth: Some(0),
        ..CborOptions::default()
    };

    let mut too_deep = vec![0x81; 65];
    too_deep.push(0xf6);

    vec![
        decode_fixture("decode array container limit", from_hex("83010203"), Some(container)),
        decode_fixture("decode text byte limit", from_hex("626162"), Some(bytes)),
        decode_fixture("decode input byte limit", from_hex("000000"), Some(bytes)),
        decode_fixture("decode excessive depth", too_deep, None),
        decode_fixture("decode invalid depth option", from_hex("00"), Some(depth)),
        decode_fixture("decode depth zero scalar", from_hex("00"), Some(zero_depth)),
        decode_fixture("decode depth zero array", from_hex("8100"), Some(zero_depth)),
        decode_fixture(
            "decode oversized byte string",
            from_hex(&format!("5a{:08x}", 16 * 1024 * 1024 + 1)),
            None,
        ),
        decode_fixture(
            "decode oversized text string",
            from_hex(&format!("7a{:08x}", 16 * 1024 * 1024 + 1)),
            None,
        ),
        decode_fixture(
            "decode oversized array",
            from_hex(&format!("9a{:08x}", 1_000_000 + 1)),
            None,
        ),
        decode_fixture(
            "decode oversized map",
            from_hex(&format!("ba{:08x}", 1_000_000 + 1)),
            None,
        ),
    ]
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = encode_vectors();
    fixtures.extend(encode_error_fixtures());
    fixtures.extend(decode_vectors());
    fixtures.extend(decode_error_fixtures());
    fixtures.extend(limit_fixtures());
    fixtures
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    for (index, fixture) in fixtures().iter().enumerate() {
        let id = u64::try_from(index + 1).expect("fixture index must fit in request id");
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);
        assert_eq!(actual, expected, "differential mismatch: {}", fixture.name);
    }
}
