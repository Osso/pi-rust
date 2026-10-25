use crate::shared_oracle as oracle;
use pi_coding_agent::core::footer_data_provider::{
    ExtensionStatus, FooterDataProvider, find_git_paths, resolve_footer_executable_name_with_env,
};
use pi_js_compat::js_json::{JsString, parse_lossless};
use serde_json::json;
#[tokio::test]
async fn footer_status_replacement_removal_and_map_order_match_frozen_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-footer-statuses-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::default(),
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let expected = oracle.request(&json!({"directory":directory.path(),"units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let z = JsString::from("z") + &raw;
        let a = JsString::from("a") + &raw;
        let provider = FooterDataProvider::new(JsString::from(directory.path().display().to_string()));
        provider.set_extension_status(&z, Some(ExtensionStatus::Text(JsString::from("first") + &raw)));
        provider.set_extension_status(&a, Some(ExtensionStatus::Text(JsString::from("second") + &raw)));
        provider.set_extension_status(&z, Some(ExtensionStatus::Text(JsString::from("updated") + &raw)));
        let mut snapshots = vec![
            provider
                .get_extension_statuses()
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
        ];
        provider.set_extension_status(&z, None);
        snapshots.push(
            provider
                .get_extension_statuses()
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
        );
        provider.clear_extension_statuses();
        snapshots.push(
            provider
                .get_extension_statuses()
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
        );
        provider.dispose();
        let actual = serde_json::to_string(&snapshots).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "raw={raw:?}"
        );
    }
}

#[tokio::test]
async fn native_footer_paths_executable_names_and_branch_js_whitespace_are_preserved() {
    let mut oracle = oracle::TsOracle::spawn("raw-footer-statuses-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let cwd = JsString::from(directory.path().display().to_string()) + "/" + &raw;
        let git_dir = cwd.clone() + "/.git";
        std::fs::create_dir_all(pi_js_compat::js_json::encode_node_utf8(&git_dir)).unwrap();
        std::fs::write(
            pi_js_compat::js_json::encode_node_utf8(&(git_dir.clone() + "/HEAD")),
            "ref: refs/heads/branch\u{85}\n",
        )
        .unwrap();
        let paths = find_git_paths(&cwd).unwrap();
        assert_eq!(paths.repo_dir, cwd);
        assert_eq!(paths.common_git_dir, git_dir);
        let provider = FooterDataProvider::new(&cwd);
        assert_eq!(provider.get_git_branch(), Some(JsString::from("branch\u{85}")));
        let branch = provider.get_git_branch();
        provider.dispose();
        let expected =
            oracle.request(&json!({"operation":"native-paths","directoryUnits":cwd.units(),"units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let source_result = serde_json::to_string(&(
            branch,
            resolve_footer_executable_name_with_env(&[JsString::from("/opt/pi-") + &raw], None),
        ))
        .unwrap();
        assert_eq!(
            parse_lossless(source_result).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap()
        );
        let executable = JsString::from("pi-") + &raw;
        assert_eq!(
            resolve_footer_executable_name_with_env(&[JsString::from("/opt/") + &executable], None),
            Some(executable)
        );
    }
}

#[test]
fn native_cwd_formatting_matches_frozen_node_paths() {
    let mut oracle = oracle::TsOracle::spawn("raw-footer-statuses-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        for (cwd, home) in [
            (JsString::from("/home/user/") + &raw, JsString::from("/home/user")),
            (
                JsString::from("/home/") + &raw + "/work",
                JsString::from("/home/") + &raw,
            ),
            (JsString::from("/else/") + &raw, JsString::from("/home/user")),
            (JsString::from("~/") + &raw, JsString::from("/")),
            (JsString::from("\u{85}") + &raw, JsString::from("/")),
        ] {
            let expected = oracle.request(&json!({"operation":"cwd","cwdUnits":cwd.units(),"homeUnits":home.units()}));
            assert_eq!(expected["ok"], true, "{expected}");
            let actual =
                pi_coding_agent::modes::interactive::components::footer::format_cwd_for_footer(&cwd, Some(&home));
            assert_eq!(
                parse_lossless(serde_json::to_string(&actual).unwrap()).unwrap(),
                parse_lossless(expected["json"].as_str().unwrap()).unwrap()
            );
        }
    }
}

#[test]
fn footer_status_trimming_does_not_treat_next_line_control_as_js_whitespace() {
    assert_eq!(
        pi_coding_agent::utils::footer_status::sanitize_footer_status_text("\u{85}"),
        "\u{85}"
    );
}

#[test]
fn native_footer_status_text_and_sorting_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-footer-statuses-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let statuses = indexmap::IndexMap::from([
            (JsString::from("z") + &raw, raw.clone() + "z\r\n\t  x"),
            (JsString::from("a") + &raw, raw.clone() + "\u{85}"),
            (JsString::from("goal"), JsString::from(" \t") + &raw + "\u{feff}"),
        ]);
        let expected = oracle.request(&json!({"operation":"helpers","units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = serde_json::to_string(
            &pi_coding_agent::utils::footer_status::format_extension_status_text_lines(&statuses),
        )
        .unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "raw={raw:?}"
        );
    }
}
