use pi_agent_core::harness::{FileInfo, FileKind};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_014;
const EXPECTED_FIXTURE_COUNT: usize = 10;

struct Fixture {
    name: &'static str,
    path: &'static str,
    kind: FileKind,
    size_bits: u64,
    mtime_ms_bits: u64,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    assert_eq!(FIXTURES.len(), EXPECTED_FIXTURE_COUNT);

    for (offset, fixture) in FIXTURES.iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&request(id, fixture));
        let actual = rust_response(id, fixture);

        assert_eq!(actual, expected, "file-info mismatch: {}", fixture.name);
    }
}

fn request(id: u64, fixture: &Fixture) -> Value {
    json!({
        "id": id,
        "operation": "file_info",
        "name": fixture.name,
        "path": fixture.path,
        "kind": fixture.kind.as_str(),
        "sizeBits": format_bits(fixture.size_bits),
        "mtimeMsBits": format_bits(fixture.mtime_ms_bits),
    })
}

fn rust_response(id: u64, fixture: &Fixture) -> Value {
    let info = FileInfo {
        name: JsString::from(fixture.name.to_owned()),
        path: JsString::from(fixture.path.to_owned()),
        kind: fixture.kind,
        size: f64::from_bits(fixture.size_bits),
        mtime_ms: f64::from_bits(fixture.mtime_ms_bits),
    };

    ok_response(
        id,
        json!({
            "name": info.name,
            "path": info.path,
            "kind": info.kind.as_str(),
            "sizeBits": format_bits(info.size.to_bits()),
            "mtimeMsBits": format_bits(info.mtime_ms.to_bits()),
        }),
    )
}

fn format_bits(bits: u64) -> String {
    format!("{bits:016x}")
}

const FIXTURES: [Fixture; EXPECTED_FIXTURE_COUNT] = [
    Fixture {
        name: "",
        path: "",
        kind: FileKind::File,
        size_bits: 0x0000_0000_0000_0000,
        mtime_ms_bits: 0x8000_0000_0000_0000,
    },
    Fixture {
        name: "README.md",
        path: "/tmp/README.md",
        kind: FileKind::Directory,
        size_bits: 0x0000_0000_0000_0001,
        mtime_ms_bits: 0x8000_0000_0000_0001,
    },
    Fixture {
        name: "名称",
        path: "/tmp/名称",
        kind: FileKind::Symlink,
        size_bits: 0x3fd5_5555_5555_5555,
        mtime_ms_bits: 0xbfd5_5555_5555_5555,
    },
    Fixture {
        name: "🙂/child",
        path: "目录/🙂/child",
        kind: FileKind::File,
        size_bits: 0x3ff0_0000_0000_0000,
        mtime_ms_bits: 0xbff0_0000_0000_0000,
    },
    Fixture {
        name: "nul\0name",
        path: "/tmp/nul\0name",
        kind: FileKind::Directory,
        size_bits: 0x433f_ffff_ffff_ffff,
        mtime_ms_bits: 0x4340_0000_0000_0000,
    },
    Fixture {
        name: "punctuation!@#$%^&*()",
        path: "C:\\tmp\\punctuation!@#$%^&*()",
        kind: FileKind::Symlink,
        size_bits: 0x4340_0000_0000_0001,
        mtime_ms_bits: 0xffef_ffff_ffff_ffff,
    },
    Fixture {
        name: "max-finite",
        path: "/numeric/max-finite",
        kind: FileKind::File,
        size_bits: 0x7fef_ffff_ffff_ffff,
        mtime_ms_bits: 0x7ff0_0000_0000_0000,
    },
    Fixture {
        name: "infinity/NaN",
        path: "/numeric/infinity/NaN",
        kind: FileKind::Directory,
        size_bits: 0xfff0_0000_0000_0000,
        mtime_ms_bits: 0x7ff8_0000_0000_0000,
    },
    Fixture {
        name: "payload\0NaN",
        path: "🙂/payload\0NaN!",
        kind: FileKind::Symlink,
        size_bits: 0x7ff8_0000_0000_0042,
        mtime_ms_bits: 0x7ff0_0000_0000_0042,
    },
    Fixture {
        name: "negative-NaN",
        path: "!@#$%^&*()/negative-NaN",
        kind: FileKind::File,
        size_bits: 0xfff8_0000_0000_0042,
        mtime_ms_bits: 0x0000_0000_0000_0000,
    },
];
