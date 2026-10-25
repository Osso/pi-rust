use crate::shared_oracle as oracle;

use pi_coding_agent::modes::interactive::theme::{
    TerminalAutoThemeDetector, TerminalBackgroundThemeDetector, TerminalTheme, ThemeQueryError,
    detect_terminal_theme_for_auto,
};
use pi_tui::terminal_colors::RgbColor;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    time::{Duration, Instant},
};

struct Detector {
    scenario: Value,
    events: RefCell<Vec<Value>>,
}
impl Detector {
    fn query(&self, name: &str, timeout: Duration) {
        self.events.borrow_mut().push(json!([name, "start"]));
        std::thread::sleep(timeout);
        self.events.borrow_mut().push(json!([name, "finish"]));
    }
}
impl TerminalBackgroundThemeDetector for Detector {
    fn query_terminal_background_color(&self, timeout: Duration) -> Result<Option<RgbColor>, ThemeQueryError> {
        self.query("background", timeout);
        if self.scenario["backgroundError"] == true {
            return Err(ThemeQueryError("background".into()));
        }
        Ok(self.scenario["background"].as_object().map(|rgb| RgbColor {
            r: rgb["r"].as_f64().unwrap(),
            g: rgb["g"].as_f64().unwrap(),
            b: rgb["b"].as_f64().unwrap(),
        }))
    }
}
impl TerminalAutoThemeDetector for Detector {
    fn query_terminal_color_scheme(&self, timeout: Duration) -> Option<Result<Option<TerminalTheme>, ThemeQueryError>> {
        if self.scenario["missingScheme"] == true {
            return None;
        }
        self.query("scheme", timeout);
        if self.scenario["schemeError"] == true {
            return Some(Err(ThemeQueryError("scheme".into())));
        }
        Some(Ok(self.scenario["scheme"].as_str().map(|theme| {
            if theme == "light" {
                TerminalTheme::Light
            } else {
                TerminalTheme::Dark
            }
        })))
    }
}

#[test]
fn auto_theme_queries_match_frozen_order_and_early_return() {
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let white = json!({"r":255,"g":255,"b":255});
    for scenario in [
        json!({"scheme":"dark","background":white}),
        json!({"scheme":null,"background":white}),
        json!({"schemeError":true,"background":white}),
        json!({"missingScheme":true,"background":white}),
        json!({"scheme":null,"background":null}),
        json!({"schemeError":true,"backgroundError":true}),
    ] {
        let request = json!({"id":1,"module":"modes/interactive/theme/theme.ts","operation":"autoThemeDetection","timeoutMs":30,"scheme":scenario["scheme"],"background":scenario["background"],"missingScheme":scenario["missingScheme"],"schemeError":scenario["schemeError"],"backgroundError":scenario["backgroundError"],"env":{"COLORFGBG":"0;15"}});
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        let detector = Detector {
            scenario: scenario.clone(),
            events: RefCell::new(Vec::new()),
        };
        let start = Instant::now();
        let theme = detect_terminal_theme_for_auto(
            &detector,
            Duration::from_millis(30),
            &BTreeMap::from([("COLORFGBG".into(), "0;15".into())]),
        );
        let elapsed = start.elapsed();
        assert_eq!(
            json!({"theme":theme.as_str(),"events":*detector.events.borrow()}),
            expected["output"],
            "{scenario}"
        );
        let queries = detector.events.borrow().len() / 2;
        assert!(
            elapsed >= Duration::from_millis(30 * queries as u64),
            "query completion precedes its timeout: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "unexpected query delay: {elapsed:?}"
        );
    }
}
