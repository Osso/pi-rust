use pi_agent_core::harness::tools::image::detect_supported_image_mime_type;
use pi_js_compat::base64::buffer_to_base64;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_069;
const EXPECTED_FIXTURE_COUNT: usize = 12;
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    bytes: Vec<u8>,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "image-helper mismatch: {}", fixture.name);
    }
}

fn assert_request_ids(fixtures: &[Fixture]) {
    for (offset, fixture) in fixtures.iter().enumerate() {
        let expected = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        assert_eq!(fixture.request_id, expected, "fixture IDs must remain contiguous");
    }
}

fn request(fixture: &Fixture) -> Value {
    json!({
        "id": fixture.request_id,
        "operation": "image_helpers",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "bytesHex": bytes_to_hex(&case.bytes),
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture
        .cases
        .iter()
        .map(|case| {
            json!({
                "name": case.name,
                "mimeType": detect_supported_image_mime_type(&case.bytes),
                "base64": buffer_to_base64(&case.bytes),
            })
        })
        .collect::<Vec<_>>();
    ok_response(fixture.request_id, json!({"results": results}))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(hex_digit(byte >> 4));
        output.push(hex_digit(byte & 0x0f));
    }
    output
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        10..=15 => char::from(b'a' + value - 10),
        _ => unreachable!("hex digit must be four bits"),
    }
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            request_id: 13_069,
            name: "empty and short prefixes",
            cases: vec![
                case("empty", vec![]),
                case("jpeg-short", vec![0xff, 0xd8]),
                case("base64-one-byte", vec![0]),
                case("base64-two-bytes", vec![0, 1]),
            ],
        },
        Fixture {
            request_id: 13_070,
            name: "jpeg precedence and near misses",
            cases: vec![
                case("jpeg-exact-prefix", vec![0xff, 0xd8, 0xff]),
                case("jpeg-ordinary-fourth-byte", vec![0xff, 0xd8, 0xff, 0x00]),
                case("jpeg-f7-rejection", vec![0xff, 0xd8, 0xff, 0xf7]),
                case("jpeg-altered-prefix", vec![0xff, 0xd9, 0xff]),
                case("jpeg-before-png-shaped-bytes", jpeg_with_png_tail()),
            ],
        },
        Fixture {
            request_id: 13_071,
            name: "png signature and structural boundaries",
            cases: vec![
                case("png-signature-only", PNG_SIGNATURE.to_vec()),
                case("png-minimal", minimal_png()),
                case("png-wrong-ihdr-length", png_with_wrong_ihdr_length()),
                case("png-wrong-ihdr-type", png_with_wrong_ihdr_type()),
                case("png-wrong-signature", png_with_wrong_signature()),
            ],
        },
        Fixture {
            request_id: 13_072,
            name: "png animation chunk order",
            cases: vec![
                case("actl-before-idat", png_with_chunks(&[(b"acTL", 0, 0)])),
                case("idat-before-actl", png_with_chunks(&[(b"IDAT", 0, 0), (b"acTL", 0, 0)])),
                case(
                    "unrelated-complete-chunk-before-idat",
                    png_with_chunks(&[(b"tEXt", 0, 0), (b"IDAT", 0, 0)]),
                ),
            ],
        },
        Fixture {
            request_id: 13_073,
            name: "png truncated and oversized chunks",
            cases: vec![
                case("huge-unrelated-chunk", png_with_chunk_header(b"tEXt", u32::MAX)),
                case("truncated-actl-type", png_with_partial_chunk_type(b"ac")),
                case("truncated-actl-header", png_with_chunk_header(b"acTL", 0)),
                case("truncated-idat-header", png_with_chunk_header(b"IDAT", 0)),
            ],
        },
        Fixture {
            request_id: 13_074,
            name: "gif and webp signatures",
            cases: vec![
                case("gif-exact", b"GIF".to_vec()),
                case("gif-longer", b"GIF89a".to_vec()),
                case("gif-case-near-miss", b"gif".to_vec()),
                case("webp-exact", webp_bytes([0, 0, 0, 0])),
                case("webp-arbitrary-size", webp_bytes([0xde, 0xad, 0xbe, 0xef])),
                case("webp-wrong-offset", b"RIFF\0\0\0\0WEBQ".to_vec()),
                case("webp-case-near-miss", b"RIFF\0\0\0\0webp".to_vec()),
                case("webp-short", b"RIFF\0\0\0\0WEB".to_vec()),
            ],
        },
        Fixture {
            request_id: 13_075,
            name: "bmp core header depths and size zero",
            cases: bmp_core_cases(),
        },
        Fixture {
            request_id: 13_076,
            name: "bmp modern minimum and declared boundaries",
            cases: vec![
                case("dib40-minimum", bmp_modern(55, 54, 40, 1, 24, 30)),
                case("dib41-minimum", bmp_modern(56, 55, 41, 1, 24, 30)),
                case("dib124-minimum", bmp_modern(139, 138, 124, 1, 24, 30)),
                case("dib40-zero-size", bmp_modern(0, 54, 40, 1, 24, 30)),
                case("dib41-zero-size", bmp_modern(0, 55, 41, 1, 24, 30)),
                case("dib124-zero-size", bmp_modern(0, 138, 124, 1, 24, 30)),
            ],
        },
        Fixture {
            request_id: 13_077,
            name: "bmp invalid size offset and dib headers",
            cases: vec![
                case("bmp-too-short", short_bmp()),
                case("declared-size-too-small", bmp_core(25, 26, 1, 24)),
                case("pixel-offset-too-small", bmp_core(0, 25, 1, 24)),
                case("modern-buffer-below-thirty", bmp_modern(0, 54, 40, 1, 24, 26)),
                case("dib-size-too-small", bmp_modern(0, 54, 39, 1, 24, 30)),
                case("dib-size-too-large", bmp_modern(0, 54, 125, 1, 24, 30)),
            ],
        },
        Fixture {
            request_id: 13_078,
            name: "bmp invalid planes depths and declared equality",
            cases: {
                let mut cases = vec![case("invalid-planes", bmp_core(0, 26, 2, 24))];
                for (name, bits_per_pixel) in [
                    ("unsupported-bpp-00", 0),
                    ("unsupported-bpp-02", 2),
                    ("unsupported-bpp-03", 3),
                    ("unsupported-bpp-05", 5),
                    ("unsupported-bpp-07", 7),
                    ("unsupported-bpp-09", 9),
                    ("unsupported-bpp-15", 15),
                    ("unsupported-bpp-17", 17),
                    ("unsupported-bpp-23", 23),
                    ("unsupported-bpp-25", 25),
                    ("unsupported-bpp-31", 31),
                    ("unsupported-bpp-33", 33),
                    ("unsupported-bpp-64", 64),
                ] {
                    cases.push(case(name, bmp_core(0, 26, 1, bits_per_pixel)));
                }
                cases.push(case("declared-size-equals-offset", bmp_modern(54, 54, 40, 1, 24, 30)));
                cases
            },
        },
        Fixture {
            request_id: 13_079,
            name: "base64 lengths alphabet and padding",
            cases: base64_cases(),
        },
        Fixture {
            request_id: 13_080,
            name: "deterministic binary corpus",
            cases: vec![
                case("sixty-four-byte-corpus", (0..64).map(|value| value as u8).collect()),
                case("all-byte-values", (0..=255).map(|value| value as u8).collect()),
                case("minimal-jpeg", vec![0xff, 0xd8, 0xff]),
                case("minimal-png", minimal_png()),
                case("minimal-bmp", bmp_core(0, 26, 1, 24)),
            ],
        },
    ]
}

fn case(name: &'static str, bytes: Vec<u8>) -> Case {
    Case { name, bytes }
}

fn minimal_png() -> Vec<u8> {
    let mut png = PNG_SIGNATURE.to_vec();
    png.extend_from_slice(&[0, 0, 0, 13]);
    png.extend_from_slice(b"IHDR");
    png
}

fn png_with_wrong_ihdr_length() -> Vec<u8> {
    let mut png = minimal_png();
    png[8..12].copy_from_slice(&12u32.to_be_bytes());
    png
}

fn png_with_wrong_ihdr_type() -> Vec<u8> {
    let mut png = minimal_png();
    png[12..16].copy_from_slice(b"iHDR");
    png
}

fn png_with_wrong_signature() -> Vec<u8> {
    let mut png = minimal_png();
    png[0] ^= 1;
    png
}

fn jpeg_with_png_tail() -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8, 0xff, 0x00];
    bytes.extend_from_slice(&PNG_SIGNATURE);
    bytes
}

fn complete_png() -> Vec<u8> {
    let mut png = PNG_SIGNATURE.to_vec();
    append_png_chunk(&mut png, b"IHDR", 13, 13);
    png
}

fn png_with_chunks(chunks: &[(&[u8; 4], u32, usize)]) -> Vec<u8> {
    let mut png = complete_png();
    for (chunk_type, chunk_length, data_length) in chunks {
        append_png_chunk(&mut png, chunk_type, *chunk_length, *data_length);
    }
    png
}

fn append_png_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], chunk_length: u32, data_length: usize) {
    png.extend_from_slice(&chunk_length.to_be_bytes());
    png.extend_from_slice(chunk_type);
    png.extend(std::iter::repeat_n(0, data_length));
    png.extend_from_slice(&[0, 0, 0, 0]);
}

fn png_with_chunk_header(chunk_type: &[u8; 4], chunk_length: u32) -> Vec<u8> {
    let mut png = complete_png();
    png.extend_from_slice(&chunk_length.to_be_bytes());
    png.extend_from_slice(chunk_type);
    png
}

fn png_with_partial_chunk_type(partial_type: &[u8]) -> Vec<u8> {
    let mut png = complete_png();
    png.extend_from_slice(&0u32.to_be_bytes());
    png.extend_from_slice(partial_type);
    png
}

fn webp_bytes(size_bytes: [u8; 4]) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&size_bytes);
    bytes.extend_from_slice(b"WEBP");
    bytes
}

fn bmp_core_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (name, bits_per_pixel) in [
        ("bpp-1", 1),
        ("bpp-4", 4),
        ("bpp-8", 8),
        ("bpp-16", 16),
        ("bpp-24", 24),
        ("bpp-32", 32),
    ] {
        cases.push(case(name, bmp_core(0, 26, 1, bits_per_pixel)));
    }
    cases.push(case("declared-size-boundary", bmp_core(27, 26, 1, 24)));
    cases
}

fn short_bmp() -> Vec<u8> {
    let mut bmp = vec![0; 25];
    bmp[0..2].copy_from_slice(b"BM");
    bmp
}

fn bmp_core(declared_size: u32, pixel_offset: u32, planes: u16, bits_per_pixel: u16) -> Vec<u8> {
    let mut bmp = vec![0; 26];
    bmp[0..2].copy_from_slice(b"BM");
    write_u32_le(&mut bmp, 2, declared_size);
    write_u32_le(&mut bmp, 10, pixel_offset);
    write_u32_le(&mut bmp, 14, 12);
    write_u16_le(&mut bmp, 22, planes);
    write_u16_le(&mut bmp, 24, bits_per_pixel);
    bmp
}

fn bmp_modern(
    declared_size: u32,
    pixel_offset: u32,
    dib_header_size: u32,
    planes: u16,
    bits_per_pixel: u16,
    length: usize,
) -> Vec<u8> {
    let mut bmp = vec![0; length];
    if length >= 2 {
        bmp[0..2].copy_from_slice(b"BM");
    }
    write_u32_le(&mut bmp, 2, declared_size);
    write_u32_le(&mut bmp, 10, pixel_offset);
    write_u32_le(&mut bmp, 14, dib_header_size);
    if length >= 30 {
        write_u16_le(&mut bmp, 26, planes);
        write_u16_le(&mut bmp, 28, bits_per_pixel);
    }
    bmp
}

fn base64_cases() -> Vec<Case> {
    let mut cases = [
        "deterministic-length-00",
        "deterministic-length-01",
        "deterministic-length-02",
        "deterministic-length-03",
        "deterministic-length-04",
        "deterministic-length-05",
        "deterministic-length-06",
        "deterministic-length-07",
        "deterministic-length-08",
        "deterministic-length-09",
        "deterministic-length-10",
        "deterministic-length-11",
        "deterministic-length-12",
    ]
    .into_iter()
    .enumerate()
    .map(|(length, name)| case(name, (0..length).map(|value| value as u8).collect()))
    .collect::<Vec<_>>();
    cases.extend([
        case("high-bit-alphabet", vec![0xfb, 0xff, 0xff]),
        case("known-binary", vec![0x00, 0x10, 0x83, 0xff, 0xfe, 0xfd, 0x7f]),
        case("padding-one", vec![0xff]),
        case("padding-two", vec![0xff, 0xfe]),
    ]);
    cases
}

fn write_u16_le(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
