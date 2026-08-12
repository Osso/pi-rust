use pi_coding_agent::modes::rpc::JsonlLineDecoder;
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 9010;

#[derive(Clone)]
enum Chunk {
    String(String),
    Bytes(Vec<u8>),
}

struct Fixture {
    name: &'static str,
    chunks: Vec<Chunk>,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    for (offset, fixture) in fixtures().into_iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&request(id, &fixture.chunks));
        let lines = decode(&fixture.chunks);
        let actual = json!({"id": id, "ok": true, "output": {"lines": lines}});

        assert_eq!(actual, expected, "JSONL decoder mismatch: {}", fixture.name);
    }
}

fn fixtures() -> Vec<Fixture> {
    vec![
        strings("LF records", &["one\ntwo\n"]),
        strings("CR and empty records", &["one\r\n\r\r\n\nbare\rinside\n"]),
        strings("unterminated final line", &["tail"]),
        strings("empty stream", &[""]),
        strings("Unicode separators", &["a\u{2028}b\nc\u{2029}d\n"]),
        strings("arbitrary ASCII chunks", &["o", "ne", "\n", "two", "\n"]),
        bytes(
            "split UTF-8 scalars",
            &[&[0xc3], &[0xa9, b'\n'], &[0xf0], &[0x90], &[0x80], &[0x80, b'\n']],
        ),
        bytes(
            "malformed UTF-8",
            &[
                &[0xff, b'\n'],
                &[0xe2, 0x28, 0xa1, b'\n'],
                &[0xc0, 0xaf, b'\n'],
                &[0xed, 0xa0, 0x80, b'\n'],
            ],
        ),
        bytes("incomplete UTF-8 at end", &[&[0xe2, 0x82]]),
        bytes("BOM and empty chunk", &[&[], &[0xef, 0xbb, 0xbf, b'x', b'\n']]),
        Fixture {
            name: "string chunk with pending bytes",
            chunks: vec![
                Chunk::Bytes(vec![0xf0]),
                Chunk::String("string line\n".to_owned()),
                Chunk::Bytes(vec![0x9f, 0x92, 0xa9, b'\n']),
            ],
        },
    ]
}

fn strings(name: &'static str, chunks: &[&str]) -> Fixture {
    Fixture {
        name,
        chunks: chunks.iter().map(|chunk| Chunk::String((*chunk).to_owned())).collect(),
    }
}

fn bytes(name: &'static str, chunks: &[&[u8]]) -> Fixture {
    Fixture {
        name,
        chunks: chunks.iter().map(|chunk| Chunk::Bytes((*chunk).to_vec())).collect(),
    }
}

fn request(id: u64, chunks: &[Chunk]) -> Value {
    let chunks = chunks.iter().map(normalize_chunk).collect::<Vec<_>>();
    json!({"id": id, "operation": "read_jsonl_sequence", "chunks": chunks})
}

fn normalize_chunk(chunk: &Chunk) -> Value {
    match chunk {
        Chunk::String(value) => json!({"kind": "string", "value": value}),
        Chunk::Bytes(value) => json!({"kind": "bytes_hex", "value": encode_hex(value)}),
    }
}

fn decode(chunks: &[Chunk]) -> Vec<String> {
    let mut decoder = JsonlLineDecoder::new();
    let mut lines = Vec::new();
    for chunk in chunks {
        match chunk {
            Chunk::String(value) => lines.extend(decoder.push_str(value)),
            Chunk::Bytes(value) => lines.extend(decoder.push_bytes(value)),
        }
    }
    lines.extend(decoder.end());
    lines
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
