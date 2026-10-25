use crate::shared_oracle as oracle;
use pi_coding_agent::modes::interactive::theme::{
    TerminalBackgroundThemeDetector, ThemeQueryError, detect_terminal_background_theme,
};
use pi_tui::terminal_colors::{RgbColor, parse_osc11_background_color};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
struct Query(Option<RgbColor>);
impl TerminalBackgroundThemeDetector for Query {
    fn query_terminal_background_color(&self, timeout: Duration) -> Result<Option<RgbColor>, ThemeQueryError> {
        assert_eq!(timeout, Duration::from_millis(100));
        Ok(self.0)
    }
}
fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| pi_js_compat::number::parse_number(value.as_str().unwrap()))
}
#[test]
fn osc_overflow_and_direct_numeric_rgb_match_frozen_detector() {
    let mut cases = vec![
        json!({"report":format!("\x1b]11;rgb:{}/0/0\x07","f".repeat(300))}),
        json!({"report":"\x1b]11;rgb:ff/ff/ff\x07"}),
        json!({"report":"invalid"}),
    ];
    for (r, g, b) in [
        (json!("NaN"), json!(0), json!(0)),
        (json!("Infinity"), json!(0), json!(0)),
        (json!("-Infinity"), json!(0), json!(0)),
        (json!(-1.5), json!(254.5), json!(1e30)),
        (json!(-0.0), json!(0.5), json!(255.75)),
    ] {
        cases.push(json!({"channels":{"r":r,"g":g,"b":b}}));
    }
    let actual:Vec<_>=cases.iter().map(|case|{
  let rgb=if let Some(report)=case["report"].as_str(){parse_osc11_background_color(report)}else{let rgb=&case["channels"];Some(RgbColor{r:number(&rgb["r"]),g:number(&rgb["g"]),b:number(&rgb["b"])})};
  let detection=detect_terminal_background_theme(&Query(rgb),Duration::from_millis(100),&BTreeMap::new());
  json!({"theme":detection.theme.as_str(),"source":detection.source.as_str(),"detail":detection.detail,"confidence":detection.confidence.as_str()})
 }).collect();
    let mut source = oracle::TsOracle::spawn("theme-rgb-oracle.mts", "packages");
    let expected: Value = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(json!(actual), expected["output"]);
}
