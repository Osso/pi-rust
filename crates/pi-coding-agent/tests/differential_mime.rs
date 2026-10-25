use crate::shared_oracle as oracle;

use pi_coding_agent::utils::mime::detect_supported_image_mime_type;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_010;
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

fn png_chunk(kind: &[u8; 4], length: u32, payload: usize) -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend(std::iter::repeat_n(0u8, payload));
    bytes
}

fn bmp(header_size: u32, planes: u16, bits_per_pixel: u16) -> Vec<u8> {
    let mut bytes = vec![0u8; 40];
    bytes[0] = b'B';
    bytes[1] = b'M';
    bytes[14..18].copy_from_slice(&header_size.to_le_bytes());
    let pixel_offset = 14 + header_size;
    bytes[10..14].copy_from_slice(&pixel_offset.to_le_bytes());
    let planes_offset = if header_size == 12 { 22 } else { 26 };
    bytes[planes_offset..planes_offset + 2].copy_from_slice(&planes.to_le_bytes());
    bytes[planes_offset + 2..planes_offset + 4].copy_from_slice(&bits_per_pixel.to_le_bytes());
    bytes
}

fn corpus() -> Vec<Vec<u8>> {
    let mut buffers = vec![
        vec![0xff, 0xd8, 0xff, 0xe0, 0x00],
        vec![0xff, 0xd8, 0xff, 0xf7],
        vec![0xff, 0xd8, 0xff],
        vec![0xff, 0xd8],
        vec![],
        png_chunk(b"IHDR", 13, 8),
        png_chunk(b"IDAT", 13, 8),
        png_chunk(b"IHDR", 12, 8),
        png_chunk(b"IHDR", 13, 4),
        PNG_SIGNATURE.to_vec(),
        b"GIF89a\x00\x00".to_vec(),
        b"GIF".to_vec(),
        b"GIF87a".to_vec(),
        b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec(),
        b"RIFF\x00\x00\x00\x00WEB".to_vec(),
        b"WEBP".to_vec(),
        b"hello".to_vec(),
        vec![0x89],
    ];

    for (header, planes, bits) in [
        (12, 1, 24),
        (40, 1, 24),
        (124, 1, 32),
        (40, 1, 1),
        (40, 1, 4),
        (40, 1, 8),
        (40, 1, 16),
        (40, 2, 24),
        (40, 1, 2),
        (13, 1, 24),
        (125, 1, 24),
    ] {
        buffers.push(bmp(header, planes, bits));
    }

    let mut short = bmp(40, 1, 24);
    short.truncate(25);
    buffers.push(short);

    let mut offset_before_header = bmp(40, 1, 24);
    offset_before_header[10..14].copy_from_slice(&10u32.to_le_bytes());
    buffers.push(offset_before_header);

    let mut offset_at_end = bmp(40, 1, 24);
    offset_at_end[2..6].copy_from_slice(&60u32.to_le_bytes());
    offset_at_end[10..14].copy_from_slice(&60u32.to_le_bytes());
    buffers.push(offset_at_end);

    let mut tiny_declared = bmp(40, 1, 24);
    tiny_declared[2..6].copy_from_slice(&25u32.to_le_bytes());
    buffers.push(tiny_declared);

    buffers
}

#[test]
fn coding_agent_mime_matches_the_pinned_typescript_source() {
    let buffers = corpus();
    let mut oracle = oracle::TsOracle::spawn("coding-agent-mime-oracle.ts", "packages/coding-agent/src/utils/mime.ts");
    let expected = oracle.request(&request(&buffers));
    let actual = response(&buffers);

    assert_eq!(actual, expected, "coding-agent mime mismatch");
}

fn request(buffers: &[Vec<u8>]) -> Value {
    let cases = buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| json!({"name": format!("case {index}"), "bytes": buffer}))
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_mime", "cases": cases})
}

fn response(buffers: &[Vec<u8>]) -> Value {
    let results = buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| {
            let value = detect_supported_image_mime_type(buffer).map_or(Value::Null, Value::String);
            json!({"name": format!("case {index}"), "value": value})
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
