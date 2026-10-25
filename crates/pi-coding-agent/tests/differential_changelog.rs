//! Differential proof for `utils/changelog.ts` plus focused parse tests.

use crate::shared_oracle as oracle;

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use pi_coding_agent::utils::changelog::{
    ChangelogEntry, compare_versions, get_new_entries, normalize_changelog_links, parse_changelog,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use oracle::TsOracle;

fn temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-changelog-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn entry(major: u64, minor: u64, patch: u64, content: &str) -> ChangelogEntry {
    ChangelogEntry {
        major,
        minor,
        patch,
        content: content.to_owned(),
    }
}

fn entry_json(entry: &ChangelogEntry) -> Value {
    json!({"major": entry.major, "minor": entry.minor, "patch": entry.patch, "content": entry.content})
}

const LINK_CASES: [&str; 12] = [
    "[Project Trust](README.md#project-trust)\n[Extensions](docs/extensions.md#project_trust)\n[Examples](examples/extensions/)\n[Root README](../../README.md#supply-chain-hardening)",
    "[#5167](https://github.com/earendil-works/pi-mono/pull/5167)\n[#4163](https://github.com/badlogic/pi-mono/issues/4163)\n[Agent README](https://github.com/badlogic/pi-mono/blob/main/packages/agent/README.md)\n[External](https://example.com/docs)\n[Local anchor](#settings)",
    "[Master](https://github.com/earendil-works/pi/tree/master/packages/coding-agent/docs)",
    "[External repo](https://github.com/other/repo/blob/main/README.md)",
    "[No link](plain text) and [empty]()",
    "[Relative up](../../../../README.md)",
    "[Query](docs/a.md?plain=1#L10)",
    "[Absolute](/packages/coding-agent/src/config.ts)",
    "[Directory](docs/)",
    "[Extensionless](docs)",
    "[Spaces](docs/my file.md)",
    "[Unicode](docs/é.md) and [image ![alt](img/a.png)](docs/x.md)",
];

#[test]
fn changelog_links_parse_and_compare_match_the_pinned_typescript_module() {
    let dir = temp_dir();
    let mut oracle = TsOracle::spawn("changelog-oracle.ts", "packages/coding-agent/src/utils/changelog.ts");

    for (index, markdown) in LINK_CASES.iter().enumerate() {
        for version in ["0.79.0", "v1.2.3"] {
            let response = oracle.request(&json!({
                "id": index,
                "op": "links",
                "markdown": markdown,
                "version": version,
            }));
            assert_eq!(response["ok"], true, "{response}");
            assert_eq!(
                json!(normalize_changelog_links(markdown, version)),
                response["output"]["markdown"],
                "link case {index} at {version}: {markdown}"
            );
        }
    }

    let changelog = [
        "# Changelog",
        "",
        "## [0.79.0] - 2026-01-02",
        "",
        "- Added trust",
        "",
        "## 0.78.1",
        "Fixed bug",
        "## Unreleased",
        "ignored",
        "## 0.78.0",
        "Older",
        "",
    ]
    .join("\n");
    let path = dir.join("CHANGELOG.md");
    fs::write(&path, &changelog).unwrap();
    let response = oracle.request(&json!({"id": 90, "op": "parse", "path": path}));
    assert_eq!(response["ok"], true, "{response}");
    let entries = parse_changelog(&path.display().to_string());
    let entries_json: Vec<Value> = entries.iter().map(entry_json).collect();
    assert_eq!(json!(entries_json), response["output"]["entries"]);

    let missing = dir.join("missing.md");
    let response = oracle.request(&json!({"id": 91, "op": "parse", "path": missing}));
    assert_eq!(response["output"]["entries"], json!([]));
    assert!(parse_changelog(&missing.display().to_string()).is_empty());

    let comparisons = [
        (entry(0, 79, 0, ""), entry(0, 78, 9, "")),
        (entry(1, 0, 0, ""), entry(1, 0, 0, "")),
        (entry(0, 1, 0, ""), entry(0, 2, 0, "")),
    ];
    for (index, (left, right)) in comparisons.iter().enumerate() {
        let response = oracle.request(&json!({
            "id": 100 + index,
            "op": "compare",
            "left": entry_json(left),
            "right": entry_json(right),
        }));
        assert_eq!(
            json!(compare_versions(left, right).signum()),
            response["output"]["comparison"],
            "comparison {index}"
        );
    }

    let all = vec![entry(0, 79, 0, "new"), entry(0, 78, 5, "mid"), entry(0, 78, 0, "old")];
    for (index, last_version) in ["0.78.5", "0.79.0", "0.77", "garbage"].iter().enumerate() {
        let response = oracle.request(&json!({
            "id": 200 + index,
            "op": "new-entries",
            "entries": all.iter().map(entry_json).collect::<Vec<_>>(),
            "lastVersion": last_version,
        }));
        let actual: Vec<Value> = get_new_entries(&all, &JsString::from(*last_version))
            .iter()
            .map(entry_json)
            .collect();
        assert_eq!(
            json!(actual),
            response["output"]["entries"],
            "new entries from {last_version}"
        );
    }
}

#[test]
fn parse_changelog_reads_sections_and_ignores_unversioned_headers() {
    let dir = temp_dir();
    let path = dir.join("CHANGELOG.md");
    fs::write(
        &path,
        "## [1.2.3] - date\nline one\nline two\n\n## Not a version\nignored\n## 1.2.2\nlast\n",
    )
    .unwrap();
    let entries = parse_changelog(&path.display().to_string());
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0], entry(1, 2, 3, "## [1.2.3] - date\nline one\nline two"));
    assert_eq!(entries[1], entry(1, 2, 2, "## 1.2.2\nlast"));
    assert_eq!(get_new_entries(&entries, &JsString::from("1.2.2")).len(), 1);
}

/// `Number("1\uD800")` is NaN, so `|| 0` makes that part zero; the other
/// parts still compare.
#[test]
fn unpaired_surrogate_version_parts_count_as_zero() {
    let all = vec![entry(0, 79, 0, "new"), entry(0, 78, 5, "mid")];
    let last_version = JsString::from("0.78.") + JsString::from_utf16(vec![u16::from(b'5'), 0xD800]);
    let contents: Vec<String> = get_new_entries(&all, &last_version)
        .into_iter()
        .map(|entry| entry.content)
        .collect();
    assert_eq!(contents, vec!["new".to_owned(), "mid".to_owned()]);
}
