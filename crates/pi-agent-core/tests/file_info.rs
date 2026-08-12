use std::fmt::Debug;

use pi_agent_core::harness::{FileInfo, FileKind};

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn file_kind_preserves_all_source_literals() {
    let kinds = [
        (FileKind::File, "file"),
        (FileKind::Directory, "directory"),
        (FileKind::Symlink, "symlink"),
    ];

    for (kind, literal) in kinds {
        assert_eq!(kind.as_str(), literal);
    }
}

#[test]
fn file_info_preserves_public_fields_and_strings() {
    let mut info = FileInfo {
        name: "名前🙂\0, punctuation!".to_owned(),
        path: "".to_owned(),
        kind: FileKind::Directory,
        size: 12.5,
        mtime_ms: -0.25,
    };

    assert_eq!(info.name, "名前🙂\0, punctuation!");
    assert_eq!(info.path, "");
    assert_eq!(info.kind, FileKind::Directory);
    assert_eq!(info.size.to_bits(), 12.5_f64.to_bits());
    assert_eq!(info.mtime_ms.to_bits(), (-0.25_f64).to_bits());

    info.name = "renamed".to_owned();
    info.path = "/tmp/renamed".to_owned();
    info.kind = FileKind::Symlink;
    info.size = 7.0;
    info.mtime_ms = 8.0;

    assert_eq!(info.name, "renamed");
    assert_eq!(info.path, "/tmp/renamed");
    assert_eq!(info.kind, FileKind::Symlink);
    assert_eq!(info.size.to_bits(), 7.0_f64.to_bits());
    assert_eq!(info.mtime_ms.to_bits(), 8.0_f64.to_bits());
    assert_debug(&info);
}

#[test]
fn file_info_supports_each_kind_in_metadata() {
    let infos = [
        FileInfo {
            name: "file".to_owned(),
            path: "/metadata/file".to_owned(),
            kind: FileKind::File,
            size: 1.0,
            mtime_ms: 2.0,
        },
        FileInfo {
            name: "directory".to_owned(),
            path: "/metadata/directory".to_owned(),
            kind: FileKind::Directory,
            size: 3.0,
            mtime_ms: 4.0,
        },
        FileInfo {
            name: "symlink".to_owned(),
            path: "/metadata/symlink".to_owned(),
            kind: FileKind::Symlink,
            size: 5.0,
            mtime_ms: 6.0,
        },
    ];

    assert_eq!(infos[0].kind.as_str(), "file");
    assert_eq!(infos[1].kind.as_str(), "directory");
    assert_eq!(infos[2].kind.as_str(), "symlink");
}

#[test]
fn file_info_preserves_all_selected_f64_bit_patterns() {
    const PLUS_ZERO: u64 = 0x0000_0000_0000_0000;
    const MINUS_ZERO: u64 = 0x8000_0000_0000_0000;
    const POSITIVE_SUBNORMAL: u64 = 0x0000_0000_0000_0001;
    const NEGATIVE_SUBNORMAL: u64 = 0x8000_0000_0000_0001;
    const POSITIVE_FRACTION: u64 = 0x3fd5_5555_5555_5555;
    const NEGATIVE_FRACTION: u64 = 0xbfd5_5555_5555_5555;
    const POSITIVE_ONE: u64 = 0x3ff0_0000_0000_0000;
    const NEGATIVE_ONE: u64 = 0xbff0_0000_0000_0000;
    const TWO_POW_53_MINUS_ONE: u64 = 0x433f_ffff_ffff_ffff;
    const TWO_POW_53: u64 = 0x4340_0000_0000_0000;
    const TWO_POW_53_PLUS_TWO: u64 = 0x4340_0000_0000_0001;
    const MAX_FINITE: u64 = 0x7fef_ffff_ffff_ffff;
    const NEGATIVE_MAX_FINITE: u64 = 0xffef_ffff_ffff_ffff;
    const POSITIVE_INFINITY: u64 = 0x7ff0_0000_0000_0000;
    const NEGATIVE_INFINITY: u64 = 0xfff0_0000_0000_0000;
    const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;
    const PAYLOAD_NAN: u64 = 0x7ff8_0000_0000_0042;
    const SIGNALING_NAN: u64 = 0x7ff0_0000_0000_0042;
    const NEGATIVE_NAN: u64 = 0xfff8_0000_0000_0042;

    let patterns = [
        (PLUS_ZERO, MINUS_ZERO),
        (POSITIVE_SUBNORMAL, NEGATIVE_SUBNORMAL),
        (POSITIVE_FRACTION, NEGATIVE_FRACTION),
        (POSITIVE_ONE, NEGATIVE_ONE),
        (TWO_POW_53_MINUS_ONE, TWO_POW_53),
        (TWO_POW_53_PLUS_TWO, NEGATIVE_MAX_FINITE),
        (MAX_FINITE, POSITIVE_INFINITY),
        (NEGATIVE_INFINITY, CANONICAL_NAN),
        (PAYLOAD_NAN, SIGNALING_NAN),
        (NEGATIVE_NAN, PLUS_ZERO),
    ];

    for (size_bits, mtime_bits) in patterns {
        let info = FileInfo {
            name: "numeric".to_owned(),
            path: "/numeric".to_owned(),
            kind: FileKind::File,
            size: f64::from_bits(size_bits),
            mtime_ms: f64::from_bits(mtime_bits),
        };

        assert_eq!(info.size.to_bits(), size_bits);
        assert_eq!(info.mtime_ms.to_bits(), mtime_bits);
    }
}
