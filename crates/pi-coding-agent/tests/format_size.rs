struct Case {
    name: &'static str,
    bytes: usize,
    expected: &'static str,
}

const CASES: [Case; 16] = [
    Case {
        name: "zero bytes",
        bytes: 0,
        expected: "0B",
    },
    Case {
        name: "one byte",
        bytes: 1,
        expected: "1B",
    },
    Case {
        name: "last byte branch value",
        bytes: 1_023,
        expected: "1023B",
    },
    Case {
        name: "one kibibyte",
        bytes: 1_024,
        expected: "1.0KB",
    },
    Case {
        name: "one byte above kibibyte",
        bytes: 1_025,
        expected: "1.0KB",
    },
    Case {
        name: "below half-up kibibyte",
        bytes: 1_279,
        expected: "1.2KB",
    },
    Case {
        name: "half-up kibibyte",
        bytes: 1_280,
        expected: "1.3KB",
    },
    Case {
        name: "above half-up kibibyte",
        bytes: 1_281,
        expected: "1.3KB",
    },
    Case {
        name: "five hundred twelve kibibytes",
        bytes: 524_288,
        expected: "512.0KB",
    },
    Case {
        name: "1023.5 kibibytes",
        bytes: 1_048_064,
        expected: "1023.5KB",
    },
    Case {
        name: "last kibibyte branch value",
        bytes: 1_048_575,
        expected: "1024.0KB",
    },
    Case {
        name: "one mebibyte",
        bytes: 1_048_576,
        expected: "1.0MB",
    },
    Case {
        name: "one byte above mebibyte",
        bytes: 1_048_577,
        expected: "1.0MB",
    },
    Case {
        name: "one and a quarter mebibytes",
        bytes: 1_310_720,
        expected: "1.3MB",
    },
    Case {
        name: "one and a half mebibytes",
        bytes: 1_572_864,
        expected: "1.5MB",
    },
    Case {
        name: "one gibibyte",
        bytes: 1_073_741_824,
        expected: "1024.0MB",
    },
];

fn assert_cases(format_size: fn(usize) -> String) {
    for case in &CASES {
        assert_eq!(format_size(case.bytes), case.expected, "{}", case.name);
    }
}

#[test]
fn direct_module_export_matches_source_cases() {
    assert_cases(pi_coding_agent::core::tools::truncate::format_size);
}

#[test]
fn tools_index_export_matches_source_cases() {
    assert_cases(pi_coding_agent::core::tools::format_size);
}

#[test]
fn crate_root_export_matches_source_cases() {
    assert_cases(pi_coding_agent::format_size);
}
