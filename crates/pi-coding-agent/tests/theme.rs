//! Behavioral tests for `modes/interactive/theme` (ported from the TypeScript
//! theme-detection, theme-export, theme-picker, and scrollbar-theme tests).
//!
//! Environment- and process-global-dependent behavior runs in child processes
//! through `support/env_probe.rs`, which also isolates the global theme state.

#[path = "support/env_probe.rs"]
mod env_probe;

use pi_js_compat::js_json::JsString;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use tempfile::TempDir;

fn fixture_package_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("package fixture dir");
    fs::create_dir(directory.path().join("theme")).expect("theme dir");
    for name in ["dark.json", "light.json"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("theme").join(name),
            directory.path().join("theme").join(name),
        )
        .expect("copy built-in theme");
    }
    directory
}

fn fixture_agent_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("agent fixture dir");
    fs::create_dir(directory.path().join("themes")).expect("themes dir");
    directory
}

fn probe_environment(package_dir: &Path, agent_dir: &Path, extra: &[(&str, OsString)]) -> Vec<(String, OsString)> {
    let mut environment = vec![
        ("PI_PACKAGE_DIR".to_owned(), package_dir.as_os_str().to_owned()),
        ("PI_CODING_AGENT_DIR".to_owned(), agent_dir.as_os_str().to_owned()),
    ];
    for (name, value) in extra {
        environment.push(((*name).to_owned(), value.clone()));
    }
    environment
}

fn run_theme_probe(name: &str, package_dir: &Path, agent_dir: &Path, extra: &[(&str, OsString)]) -> Output {
    let environment = probe_environment(package_dir, agent_dir, extra);
    let environment: Vec<(&str, OsString)> = environment
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    env_probe::run_probe(name, &environment)
}

fn read_json_file(path: &Path) -> serde_json::Value {
    let content = fs::read_to_string(path).expect("probe output must exist");
    serde_json::from_str(&content).expect("probe output must be JSON")
}

#[test]
fn detects_terminal_background_and_theme_settings() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    run_theme_probe("theme_detection_probe", package.path(), agent.path(), &[]);
}

#[test]
#[ignore = "child process probe"]
fn theme_detection_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use std::collections::BTreeMap;
    use std::time::Duration;

    use pi_coding_agent::modes::interactive::theme::{
        TerminalAutoThemeDetector, TerminalBackgroundThemeDetector, TerminalTheme, TerminalThemeDetection,
        ThemeDetectionConfidence, ThemeDetectionSource, ThemeQueryError, detect_terminal_background_from_env,
        detect_terminal_background_theme, detect_terminal_theme_for_auto, get_theme_for_rgb_color,
        parse_auto_theme_setting, resolve_theme_setting,
    };
    use pi_tui::terminal_colors::RgbColor;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn detection(
        theme: TerminalTheme,
        source: ThemeDetectionSource,
        detail: &str,
        high: bool,
    ) -> TerminalThemeDetection {
        TerminalThemeDetection {
            theme,
            source,
            detail: detail.to_owned(),
            confidence: if high {
                ThemeDetectionConfidence::High
            } else {
                ThemeDetectionConfidence::Low
            },
        }
    }

    assert_eq!(
        detect_terminal_background_from_env(&env(&[("COLORFGBG", "0;15")])),
        detection(
            TerminalTheme::Light,
            ThemeDetectionSource::ColorFgBg,
            "background color index 15",
            true
        )
    );
    assert_eq!(
        detect_terminal_background_from_env(&env(&[("COLORFGBG", "15;0")])),
        detection(
            TerminalTheme::Dark,
            ThemeDetectionSource::ColorFgBg,
            "background color index 0",
            true
        )
    );
    assert_eq!(
        detect_terminal_background_from_env(&env(&[("COLORFGBG", "0;7;15")])).theme,
        TerminalTheme::Light
    );
    assert_eq!(
        detect_terminal_background_from_env(&env(&[])),
        detection(
            TerminalTheme::Dark,
            ThemeDetectionSource::Fallback,
            "no terminal background hint found",
            false
        )
    );

    struct Query {
        color: Option<RgbColor>,
        error: bool,
        color_scheme: Option<Result<Option<TerminalTheme>, ThemeQueryError>>,
        queried_timeout: std::cell::Cell<u64>,
    }

    impl TerminalBackgroundThemeDetector for Query {
        fn query_terminal_background_color(&self, timeout: Duration) -> Result<Option<RgbColor>, ThemeQueryError> {
            self.queried_timeout.set(timeout.as_millis() as u64);
            if self.error {
                return Err(ThemeQueryError("terminal write failed".to_owned()));
            }
            Ok(self.color)
        }
    }

    impl TerminalAutoThemeDetector for Query {
        fn query_terminal_color_scheme(
            &self,
            _timeout: Duration,
        ) -> Option<Result<Option<TerminalTheme>, ThemeQueryError>> {
            self.color_scheme.clone()
        }
    }

    let queried = Query {
        color: Some(RgbColor {
            r: 250.0,
            g: 250.0,
            b: 250.0,
        }),
        error: false,
        color_scheme: None,
        queried_timeout: std::cell::Cell::new(0),
    };
    let result = detect_terminal_background_theme(&queried, Duration::from_millis(250), &env(&[("COLORFGBG", "15;0")]));
    assert_eq!(queried.queried_timeout.get(), 250);
    assert_eq!(
        result,
        detection(
            TerminalTheme::Light,
            ThemeDetectionSource::TerminalBackground,
            "OSC 11 background rgb(250, 250, 250)",
            true
        )
    );

    let no_color = Query {
        color: None,
        error: false,
        color_scheme: None,
        queried_timeout: std::cell::Cell::new(0),
    };
    assert_eq!(
        detect_terminal_background_theme(&no_color, Duration::from_millis(250), &env(&[("COLORFGBG", "15;0")])),
        detection(
            TerminalTheme::Dark,
            ThemeDetectionSource::ColorFgBg,
            "background color index 0",
            true
        )
    );

    let failing = Query {
        color: None,
        error: true,
        color_scheme: None,
        queried_timeout: std::cell::Cell::new(0),
    };
    assert_eq!(
        detect_terminal_background_theme(&failing, Duration::from_millis(250), &env(&[("COLORFGBG", "0;15")])),
        detection(
            TerminalTheme::Light,
            ThemeDetectionSource::ColorFgBg,
            "background color index 15",
            true
        )
    );

    let scheme_dark = Query {
        color: None,
        error: false,
        color_scheme: Some(Ok(Some(TerminalTheme::Dark))),
        queried_timeout: std::cell::Cell::new(0),
    };
    assert_eq!(
        detect_terminal_theme_for_auto(&scheme_dark, Duration::from_millis(100), &env(&[("COLORFGBG", "0;15")])),
        TerminalTheme::Dark
    );

    let scheme_failing = Query {
        color: Some(RgbColor {
            r: 250.0,
            g: 250.0,
            b: 250.0,
        }),
        error: false,
        color_scheme: Some(Err(ThemeQueryError("color-scheme query failed".to_owned()))),
        queried_timeout: std::cell::Cell::new(0),
    };
    assert_eq!(
        detect_terminal_theme_for_auto(&scheme_failing, Duration::from_millis(100), &env(&[])),
        TerminalTheme::Light
    );

    assert_eq!(
        get_theme_for_rgb_color(RgbColor { r: 8.0, g: 8.0, b: 8.0 }),
        TerminalTheme::Dark
    );
    assert_eq!(
        get_theme_for_rgb_color(RgbColor {
            r: 250.0,
            g: 250.0,
            b: 250.0
        }),
        TerminalTheme::Light
    );

    assert_eq!(
        parse_auto_theme_setting(Some(&JsString::from("light/dark"))),
        Some(pi_coding_agent::modes::interactive::theme::AutoThemeSetting {
            light_theme: JsString::from("light"),
            dark_theme: JsString::from("dark"),
        })
    );
    assert_eq!(
        resolve_theme_setting(Some(&JsString::from("dark")), TerminalTheme::Light).as_ref(),
        Some(&JsString::from("dark"))
    );
    assert_eq!(
        resolve_theme_setting(Some(&JsString::from("light/dark")), TerminalTheme::Light).as_ref(),
        Some(&JsString::from("light"))
    );
    assert_eq!(
        resolve_theme_setting(Some(&JsString::from("light/dark")), TerminalTheme::Dark).as_ref(),
        Some(&JsString::from("dark"))
    );
    assert_eq!(
        resolve_theme_setting(Some(&JsString::from("light/dark/extra")), TerminalTheme::Dark),
        None
    );
}

#[test]
fn resolves_theme_export_colors() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    let dark: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json")).unwrap(),
    )
    .unwrap();
    let mut vars = dark["vars"].clone();
    vars["pageBgVar"] = serde_json::json!("#112233");
    vars["pageBgAlias"] = serde_json::json!("pageBgVar");
    vars["infoBgVar"] = serde_json::json!("#445566");
    vars["cardBgVar"] = serde_json::json!("#223344");
    let mut custom = dark.clone();
    custom["name"] = serde_json::json!("custom-export-vars");
    custom["vars"] = vars;
    custom["export"] = serde_json::json!({
        "pageBg": "pageBgAlias",
        "cardBg": "cardBgVar",
        "infoBg": "infoBgVar",
    });
    fs::write(
        agent.path().join("themes/custom-export-vars.json"),
        serde_json::to_string(&custom).unwrap(),
    )
    .unwrap();

    let mut recursive_vars = dark["vars"].clone();
    recursive_vars["deepPageBg"] = serde_json::json!("#abcdef");
    recursive_vars["pageBgAlias"] = serde_json::json!("deepPageBg");
    recursive_vars["cardBgAnsi"] = serde_json::json!(24);
    let mut recursive = dark.clone();
    recursive["name"] = serde_json::json!("custom-export-recursive");
    recursive["vars"] = recursive_vars;
    recursive["export"] = serde_json::json!({
        "pageBg": "pageBgAlias",
        "cardBg": "cardBgAnsi",
        "infoBg": "",
    });
    fs::write(
        agent.path().join("themes/custom-export-recursive.json"),
        serde_json::to_string(&recursive).unwrap(),
    )
    .unwrap();

    run_theme_probe("theme_export_probe", package.path(), agent.path(), &[]);
}

#[test]
#[ignore = "child process probe"]
fn theme_export_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::modes::interactive::theme::{ThemeExportColors, get_theme_export_colors};

    assert_eq!(
        get_theme_export_colors(Some(&JsString::from("custom-export-vars"))),
        ThemeExportColors {
            page_bg: Some(JsString::from("#112233")),
            card_bg: Some(JsString::from("#223344")),
            info_bg: Some(JsString::from("#445566")),
        }
    );
    assert_eq!(
        get_theme_export_colors(Some(&JsString::from("custom-export-recursive"))),
        ThemeExportColors {
            page_bg: Some(JsString::from("#abcdef")),
            card_bg: Some(JsString::from("#005f87")),
            info_bg: None,
        }
    );
    assert_eq!(
        get_theme_export_colors(Some(&JsString::from("dark"))),
        ThemeExportColors {
            page_bg: Some(JsString::from("#18181e")),
            card_bg: Some(JsString::from("#1e1e24")),
            info_bg: Some(JsString::from("#3c3728")),
        }
    );
    assert_eq!(
        get_theme_export_colors(Some(&JsString::from("does-not-exist"))),
        ThemeExportColors::default()
    );
}

#[test]
fn lists_themes_by_content_name() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    let dark: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json")).unwrap(),
    )
    .unwrap();
    let mut custom = dark.clone();
    custom["name"] = serde_json::json!("bar");
    let theme_path = agent.path().join("themes/foo.json");
    fs::write(&theme_path, serde_json::to_string(&custom).unwrap()).unwrap();
    fs::write(agent.path().join("themes/broken.json"), "{ not json").unwrap();

    let output_path = agent.path().join("available.json");
    run_theme_probe(
        "theme_picker_probe",
        package.path(),
        agent.path(),
        &[("THEME_PROBE_OUT", output_path.as_os_str().to_owned())],
    );
    let available = read_json_file(&output_path);
    let names: Vec<&str> = available["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(names, ["bar", "dark", "light"]);
    assert!(!names.contains(&"foo"));
    let with_paths = available["withPaths"].as_array().unwrap();
    assert!(with_paths.contains(&serde_json::json!({"name": "bar", "path": theme_path})));
    assert!(!with_paths.iter().any(|entry| entry["name"] == "foo"));
}

#[test]
#[ignore = "child process probe"]
fn theme_picker_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::modes::interactive::theme::{get_available_themes, get_available_themes_with_paths};

    let output_path = std::env::var("THEME_PROBE_OUT").expect("THEME_PROBE_OUT");
    let with_paths: Vec<serde_json::Value> = get_available_themes_with_paths()
        .expect("available themes")
        .into_iter()
        .map(|info| {
            serde_json::json!({
                "name": info.name,
                "path": info.path,
            })
        })
        .collect();
    let output = serde_json::json!({
        "names": get_available_themes().expect("available themes"),
        "withPaths": with_paths,
    });
    fs::write(output_path, serde_json::to_string(&output).unwrap()).expect("write probe output");
}

#[test]
fn loads_color_modes_and_global_theme_state() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    run_theme_probe("theme_global_probe", package.path(), agent.path(), &[]);
}

#[test]
#[ignore = "child process probe"]
fn theme_global_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use std::cell::Cell;
    use std::rc::Rc;

    use pi_coding_agent::modes::interactive::theme::{
        ColorMode, ThemeColor, current_theme, get_theme_by_name, init_theme, on_theme_change, set_theme,
        set_theme_instance,
    };
    use pi_tui::terminal_image::{TerminalCapabilities, reset_capabilities_cache, set_capabilities};

    reset_capabilities_cache();
    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: false,
        hyperlinks: false,
    });
    let ansi256_theme = get_theme_by_name(&JsString::from("dark")).expect("dark theme");
    assert_eq!(ansi256_theme.get_color_mode(), ColorMode::Color256);
    assert!(ansi256_theme.get_fg_ansi(ThemeColor::Accent).starts_with("\x1b[38;5;"));

    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: false,
    });
    let truecolor_theme = get_theme_by_name(&JsString::from("dark")).expect("dark theme");
    assert_eq!(truecolor_theme.get_color_mode(), ColorMode::Truecolor);
    assert!(
        truecolor_theme
            .get_fg_ansi(ThemeColor::Accent)
            .starts_with("\x1b[38;2;")
    );

    init_theme(Some(&JsString::from("missing-theme")), false).expect("dark fallback");
    assert_eq!(current_theme().name.as_ref(), Some(&JsString::from("dark")));

    let changes = Rc::new(Cell::new(0));
    let counter = Rc::clone(&changes);
    on_theme_change(Rc::new(move || counter.set(counter.get() + 1)));
    let result = set_theme(&JsString::from("light"), false).expect("light theme");
    assert!(result.success);
    assert_eq!(result.error, None);
    assert_eq!(current_theme().name.as_ref(), Some(&JsString::from("light")));
    assert_eq!(changes.get(), 1);

    let failure = set_theme(&JsString::from("does-not-exist"), false).expect("dark fallback");
    assert!(!failure.success);
    assert_eq!(
        failure.error.as_ref(),
        Some(&JsString::from("Theme not found: does-not-exist"))
    );
    assert_eq!(current_theme().name.as_ref(), Some(&JsString::from("dark")));

    let in_memory = std::sync::Arc::new(
        get_theme_by_name(&JsString::from("light"))
            .expect("light theme")
            .as_ref()
            .clone(),
    );
    set_theme_instance(in_memory);
    assert_eq!(current_theme().name.as_ref(), Some(&JsString::from("light")));

    let slash = set_theme(&JsString::from("bad/name"), false).expect("dark fallback");
    assert!(!slash.success);
    assert_eq!(slash.error.as_ref(), Some(&JsString::from("Theme not found: bad/name")));
}

#[test]
fn resolves_theme_colors_in_key_order() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    let output_path = agent.path().join("resolved.json");
    run_theme_probe(
        "theme_resolved_probe",
        package.path(),
        agent.path(),
        &[("THEME_PROBE_OUT", output_path.as_os_str().to_owned())],
    );
    let resolved = read_json_file(&output_path);
    let pairs = resolved["dark"].as_array().unwrap();
    let first = pairs[0].as_array().unwrap();
    assert_eq!(first[0], "accent");
    assert_eq!(first[1], "#8abeb7");
    let find = |key: &str| {
        pairs
            .iter()
            .find(|pair| pair.as_array().unwrap()[0] == key)
            .map(|pair| pair.as_array().unwrap()[1].as_str().unwrap().to_owned())
    };
    assert_eq!(find("thinkingMax"), None);
    assert_eq!(find("scrollbarThumb"), None);
    assert_eq!(find("searchMatchBg"), None);
    assert_eq!(find("searchMatchText"), None);
    assert_eq!(find("bashMode").as_deref(), Some("#b5bd68"));
    assert_eq!(pairs.len(), 51);
}

#[test]
#[ignore = "child process probe"]
fn theme_resolved_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::modes::interactive::theme::get_resolved_theme_colors;

    let output_path = std::env::var("THEME_PROBE_OUT").expect("THEME_PROBE_OUT");
    let output = serde_json::json!({
        "dark": get_resolved_theme_colors(Some(&JsString::from("dark"))).unwrap(),
    });
    fs::write(output_path, serde_json::to_string(&output).unwrap()).expect("write probe output");
}

#[test]
fn validates_theme_json_and_names() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    run_theme_probe("theme_validation_probe", package.path(), agent.path(), &[]);
}

#[test]
#[ignore = "child process probe"]
fn theme_validation_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::modes::interactive::theme::load_theme_from_path;

    let write = |content: &str| {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("theme.json");
        fs::write(&path, content).unwrap();
        (directory, path)
    };

    let (_directory, path) = write("{\"name\":\"x\"}");
    let error = load_theme_from_path(
        &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.starts_with("Invalid theme \""));
    assert!(error.ends_with("\n\nOther errors:\n  - /: must have required properties colors"));
    assert!(!error.contains("Missing required color tokens"));

    let (_directory, path) = write("{\"name\":\"x\",\"colors\":{}}");
    let error = load_theme_from_path(
        &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("Missing required color tokens:\n  - accent\n"));
    assert!(!error.contains("Other errors"));

    let (_directory, path) = write("[]");
    assert!(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
            None
        )
        .unwrap_err()
        .to_string()
        .contains("/: must be object")
    );

    let dark: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            std::env::var("PI_PACKAGE_DIR")
                .map(PathBuf::from)
                .unwrap()
                .join("theme/dark.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut slash = dark.clone();
    slash["name"] = serde_json::json!("a/b");
    let (_directory, path) = write(&serde_json::to_string(&slash).unwrap());
    assert_eq!(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
            None
        )
        .unwrap_err()
        .to_string(),
        "Invalid theme name \"a/b\": theme names cannot contain \"/\" because it is reserved for automatic light/dark theme settings."
    );

    let mut bad = dark.clone();
    bad["name"] = serde_json::json!("bad-hex");
    bad["colors"]["extraColor"] = serde_json::json!("#fff");
    let (_directory, path) = write(&serde_json::to_string(&bad).unwrap());
    assert_eq!(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
            None
        )
        .unwrap_err()
        .to_string(),
        "Invalid hex color: #fff"
    );

    let mut circular = dark.clone();
    circular["name"] = serde_json::json!("circular");
    circular["vars"] = serde_json::json!({"a": "b", "b": "a"});
    circular["colors"]["accent"] = serde_json::json!("a");
    let (_directory, path) = write(&serde_json::to_string(&circular).unwrap());
    assert_eq!(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
            None
        )
        .unwrap_err()
        .to_string(),
        "Circular variable reference detected: a"
    );

    let mut missing_var = dark.clone();
    missing_var["name"] = serde_json::json!("missing-var");
    missing_var["colors"]["accent"] = serde_json::json!("doesnotexist");
    let (_directory, path) = write(&serde_json::to_string(&missing_var).unwrap());
    assert_eq!(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
            None
        )
        .unwrap_err()
        .to_string(),
        "Variable reference not found: doesnotexist"
    );
}

#[test]
fn maps_file_paths_to_languages() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    run_theme_probe("theme_language_probe", package.path(), agent.path(), &[]);
}

#[test]
#[ignore = "child process probe"]
fn theme_language_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::modes::interactive::theme::{get_language_from_path, is_light_theme};

    assert_eq!(
        get_language_from_path(&JsString::from("src/main.rs")),
        Some("rust".into())
    );
    assert_eq!(
        get_language_from_path(&JsString::from("src/app.TSX")),
        Some("typescript".into())
    );
    assert_eq!(
        get_language_from_path(&JsString::from("Makefile")),
        Some("makefile".into())
    );
    assert_eq!(
        get_language_from_path(&JsString::from("Dockerfile")),
        Some("dockerfile".into())
    );
    assert_eq!(get_language_from_path(&JsString::from("notes.unknown")), None);
    assert_eq!(get_language_from_path(&JsString::from("no-extension")), None);
    assert_eq!(get_language_from_path(&JsString::from("trailing.")), None);
    assert!(is_light_theme(Some(&JsString::from("light"))));
    assert!(!is_light_theme(Some(&JsString::from("dark"))));
    assert!(!is_light_theme(None));
}

#[test]
fn missing_builtin_themes_return_errors() {
    let package = tempfile::tempdir().expect("package fixture dir");
    let agent = fixture_agent_dir();
    run_theme_probe("theme_missing_builtin_probe", package.path(), agent.path(), &[]);
}

#[test]
#[ignore = "child process probe"]
fn theme_missing_builtin_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::modes::interactive::theme::{
        ThemeError, get_available_themes, get_available_themes_with_paths, get_theme_by_name, init_theme, set_theme,
        try_current_theme,
    };

    let package_dir = std::env::var("PI_PACKAGE_DIR").expect("PI_PACKAGE_DIR");
    let expected = format!("ENOENT: no such file or directory, open '{package_dir}/theme/dark.json'");

    assert_eq!(init_theme(None, false).unwrap_err().to_string(), expected);
    assert_eq!(get_available_themes().unwrap_err().to_string(), expected);
    assert_eq!(get_available_themes_with_paths().unwrap_err().to_string(), expected);
    // `getThemeByName` catches, so it returns undefined instead of throwing.
    assert!(get_theme_by_name(&JsString::from("dark")).is_none());
    // `setTheme` catches the requested theme, then the dark fallback re-throws.
    assert_eq!(
        set_theme(&JsString::from("light"), false).unwrap_err().to_string(),
        expected
    );
    assert_eq!(
        try_current_theme().unwrap_err(),
        ThemeError::NotInitialized,
        "failed init must not install a theme"
    );
    assert_eq!(
        ThemeError::NotInitialized.to_string(),
        "Theme not initialized. Call initTheme() first."
    );
}
