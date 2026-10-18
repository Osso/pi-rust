use pi_protocol::{
    CborCodecError, CborOptions, CborValue, DEFAULT_MAX_CBOR_BYTE_LENGTH, DEFAULT_MAX_CBOR_CONTAINER_LENGTH,
    DEFAULT_MAX_CBOR_DEPTH, decode_cbor, encode_cbor,
};

fn from_hex(hex: &str) -> Vec<u8> {
    assert!(hex.len().is_multiple_of(2), "hex fixture must contain whole bytes");
    (0..hex.len() / 2)
        .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("hex fixture must be valid"))
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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

fn cbor_error(result: Result<impl std::fmt::Debug, CborCodecError>) -> String {
    match result {
        Ok(value) => panic!("expected a CBOR error, got {value:?}"),
        Err(CborCodecError::Cbor(error)) => error.to_string(),
        Err(CborCodecError::Options(error)) => panic!("expected a CBOR error, got options error: {error}"),
    }
}

#[test]
fn encodes_and_decodes_rfc_8949_vectors() {
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
        (text(""), "60"),
        (text("IETF"), "6449455446"),
        (text("ü"), "62c3bc"),
        (text("水"), "63e6b0b4"),
        (text("𐅑"), "64f0908591"),
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
    ];

    for (value, wire) in vectors {
        assert_eq!(to_hex(&encode_cbor(&value, None).expect("vector must encode")), wire);
        let decoded = decode_cbor(&from_hex(wire), None).expect("vector must decode");
        if let CborValue::Number(value) = value {
            assert_eq!(decoded, CborValue::Number(value), "vector {wire}");
        } else {
            assert_eq!(decoded, value, "vector {wire}");
        }
    }
}

#[test]
fn decodes_negative_zero_with_its_sign() {
    let decoded = decode_cbor(&from_hex("fb8000000000000000"), None).expect("negative zero must decode");
    let CborValue::Number(value) = decoded else {
        panic!("negative zero must decode to a number");
    };
    assert_eq!(value.to_bits(), (-0.0f64).to_bits());
}

#[test]
fn omits_undefined_map_properties_without_omitting_falsey_values() {
    let value = map(vec![
        ("zero", number(0.0)),
        ("empty", text("")),
        ("no", CborValue::Bool(false)),
        ("nil", CborValue::Null),
    ]);
    assert_eq!(decode_cbor(&encode_cbor(&value, None).unwrap(), None).unwrap(), value);
}

#[test]
fn preserves_a_leading_unicode_bom_and_treats_proto_as_data() {
    assert_eq!(decode_cbor(&from_hex("63efbbbf"), None).unwrap(), text("\u{feff}"));
    let value = map(vec![("__proto__", text("safe"))]);
    assert_eq!(decode_cbor(&encode_cbor(&value, None).unwrap(), None).unwrap(), value);
}

#[test]
fn rejects_unsupported_encoder_values() {
    for value in [
        number(f64::NAN),
        number(f64::INFINITY),
        number(f64::NEG_INFINITY),
        number(9_007_199_254_740_992.0),
        number(-9_007_199_254_740_992.0),
    ] {
        cbor_error(encode_cbor(&value, None));
    }
}

#[test]
fn rejects_excessive_encoder_depth() {
    let mut too_deep = CborValue::Null;
    for _ in 0..=DEFAULT_MAX_CBOR_DEPTH {
        too_deep = array(vec![too_deep]);
    }
    let message = cbor_error(encode_cbor(&too_deep, None));
    assert!(message.to_lowercase().contains("depth"), "{message}");
}

#[test]
fn rejects_invalid_decoder_input() {
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

    for (label, wire) in wires {
        cbor_error(decode_cbor(&from_hex(wire), None));
        assert!(!label.is_empty());
    }
}

#[test]
fn enforces_depth_and_declared_length_limits_before_traversing_values() {
    let mut too_deep = vec![0x81; usize::from(DEFAULT_MAX_CBOR_DEPTH) + 1];
    too_deep.push(0xf6);
    let message = cbor_error(decode_cbor(&too_deep, None));
    assert!(message.to_lowercase().contains("depth"), "{message}");

    let oversized_bytes = format!("5a{:08x}", DEFAULT_MAX_CBOR_BYTE_LENGTH + 1);
    let oversized_text = format!("7a{:08x}", DEFAULT_MAX_CBOR_BYTE_LENGTH + 1);
    let oversized_array = format!("9a{:08x}", DEFAULT_MAX_CBOR_CONTAINER_LENGTH + 1);
    let oversized_map = format!("ba{:08x}", DEFAULT_MAX_CBOR_CONTAINER_LENGTH + 1);
    for wire in [oversized_bytes, oversized_text, oversized_array, oversized_map] {
        let message = cbor_error(decode_cbor(&from_hex(&wire), None));
        assert!(message.to_lowercase().contains("limit"), "{message}");
    }
}

#[test]
fn supports_stricter_caller_provided_limits() {
    let container = CborOptions {
        max_container_length: Some(2),
        ..CborOptions::default()
    };
    let message = cbor_error(decode_cbor(&from_hex("83010203"), Some(container)));
    assert!(message.to_lowercase().contains("limit"), "{message}");
    let message = cbor_error(encode_cbor(
        &array(vec![number(1.0), number(2.0), number(3.0)]),
        Some(container),
    ));
    assert!(message.to_lowercase().contains("limit"), "{message}");

    let bytes = CborOptions {
        max_byte_length: Some(2),
        ..CborOptions::default()
    };
    let message = cbor_error(decode_cbor(&from_hex("626162"), Some(bytes)));
    assert!(message.to_lowercase().contains("limit"), "{message}");
    let message = cbor_error(encode_cbor(&text("ab"), Some(bytes)));
    assert!(message.to_lowercase().contains("limit"), "{message}");
}

#[test]
fn rejects_depth_above_the_configurable_maximum() {
    let options = CborOptions {
        max_depth: Some(513),
        ..CborOptions::default()
    };
    assert!(matches!(
        decode_cbor(&from_hex("00"), Some(options)),
        Err(CborCodecError::Options(_))
    ));
    assert!(matches!(
        encode_cbor(&number(1.0), Some(options)),
        Err(CborCodecError::Options(_))
    ));
}

#[test]
fn supports_configured_depth_zero() {
    let options = CborOptions {
        max_depth: Some(0),
        ..CborOptions::default()
    };
    assert_eq!(decode_cbor(&from_hex("00"), Some(options)).unwrap(), number(0.0));
    assert!(encode_cbor(&array(vec![number(1.0)]), Some(options)).is_err());
}

#[test]
fn rejects_input_longer_than_the_configured_byte_limit() {
    let options = CborOptions {
        max_byte_length: Some(2),
        ..CborOptions::default()
    };
    let message = cbor_error(decode_cbor(&from_hex("000000"), Some(options)));
    assert!(message.to_lowercase().contains("limit"), "{message}");
}

#[test]
fn rejects_duplicate_map_keys_in_typed_values() {
    let value = map(vec![("a", number(1.0)), ("a", number(2.0))]);
    let message = cbor_error(encode_cbor(&value, None));
    assert!(message.to_lowercase().contains("duplicate"), "{message}");
}

#[test]
fn default_limits_match_the_source_schema() {
    assert_eq!(DEFAULT_MAX_CBOR_BYTE_LENGTH, 16 * 1024 * 1024);
    assert_eq!(DEFAULT_MAX_CBOR_CONTAINER_LENGTH, 1_000_000);
    assert_eq!(DEFAULT_MAX_CBOR_DEPTH, 64);
}
