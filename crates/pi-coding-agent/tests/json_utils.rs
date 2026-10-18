use pi_coding_agent::utils::json::strip_json_comments;

struct Case {
    name: &'static str,
    input: &'static str,
    expected: &'static str,
}

const CASES: [Case; 14] = [
    Case {
        name: "line comment removed",
        input: "{\"a\":1,//c\n\"b\":2}",
        expected: "{\"a\":1,\n\"b\":2}",
    },
    Case {
        name: "only comment",
        input: "// only comment",
        expected: "",
    },
    Case {
        name: "trailing comma in object",
        input: "{\"a\":1,}",
        expected: "{\"a\":1}",
    },
    Case {
        name: "trailing comma in nested array and object",
        input: "{\"a\":[1,2,],}",
        expected: "{\"a\":[1,2]}",
    },
    Case {
        name: "trailing comma keeps surrounding whitespace",
        input: "[1 , \n ]",
        expected: "[1  \n ]",
    },
    Case {
        name: "comment-looking text inside a string is preserved",
        input: "{\"url\":\"http://x//y\"}",
        expected: "{\"url\":\"http://x//y\"}",
    },
    Case {
        name: "comma-bracket inside a string is preserved",
        input: "{\"s\":\",}\"}",
        expected: "{\"s\":\",}\"}",
    },
    Case {
        name: "escaped quote inside a string is preserved",
        input: "{\"s\":\"a\\\"//b\"}",
        expected: "{\"s\":\"a\\\"//b\"}",
    },
    Case {
        name: "escaped backslash before quote is preserved",
        input: "{\"s\":\"a\\\\\\\\\",\"x\":1}",
        expected: "{\"s\":\"a\\\\\\\\\",\"x\":1}",
    },
    Case {
        name: "comment after a string",
        input: "{\"a\":\"//\"}//tail",
        expected: "{\"a\":\"//\"}",
    },
    Case {
        name: "comment with trailing carriage return",
        input: "{\"a\":1,//c\r\n\"b\":2}",
        expected: "{\"a\":1,\n\"b\":2}",
    },
    Case {
        name: "tab whitespace before closing bracket",
        input: "[1,\t]",
        expected: "[1\t]",
    },
    Case {
        name: "multiple trailing commas",
        input: "{\"a\":1,,\n}",
        expected: "{\"a\":1,\n}",
    },
    Case {
        name: "string at end of input",
        input: "\"unterminated",
        expected: "\"unterminated",
    },
];

#[test]
fn matches_source_strip_json_comments_cases() {
    for case in &CASES {
        assert_eq!(strip_json_comments(case.input), case.expected, "{}", case.name);
    }
}
