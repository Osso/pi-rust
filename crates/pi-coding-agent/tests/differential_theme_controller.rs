use crate::shared_oracle as oracle;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions, lock_settings_manager};
use pi_coding_agent::modes::interactive::theme::{
    InteractiveThemeController, TerminalTheme, ThemeQueryError, ThemeUi, current_theme, get_theme_by_name,
};
use pi_js_compat::js_json::{JsObject, JsString};
use pi_tui::terminal_colors::RgbColor;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

type Listener = Rc<dyn Fn(TerminalTheme)>;
struct Ui {
    events: Rc<RefCell<Vec<Value>>>,
    listener: RefCell<Option<Listener>>,
    scheme: Option<TerminalTheme>,
    background: Option<RgbColor>,
}
impl ThemeUi for Ui {
    fn invalidate(&self) {
        self.events.borrow_mut().push(json!(["invalidate"]));
    }
    fn request_render(&self) {
        self.events.borrow_mut().push(json!(["render"]));
    }
    fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
        self.events.borrow_mut().push(json!(["notifications", enabled]));
    }
    fn on_terminal_color_scheme_change(&self, listener: Listener) -> usize {
        *self.listener.borrow_mut() = Some(listener);
        self.events.borrow_mut().push(json!(["listen"]));
        0
    }
    fn query_terminal_background_color(&self, _timeout: Duration) -> Result<Option<RgbColor>, ThemeQueryError> {
        self.events.borrow_mut().push(json!(["background"]));
        Ok(self.background)
    }
    fn query_terminal_color_scheme(
        &self,
        _timeout: Duration,
    ) -> Option<Result<Option<TerminalTheme>, ThemeQueryError>> {
        self.events.borrow_mut().push(json!(["scheme"]));
        Some(Ok(self.scheme))
    }
}

#[test]
fn controller_settings_preview_notifications_and_errors_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let scenarios = [
        json!({"setting":"light/dark","scheme":"light","background":{"r":0,"g":0,"b":0},"steps":[{"op":"apply"},{"op":"emit","theme":"dark"},{"op":"emit","theme":"dark"},{"op":"preview","name":"light/dark"},{"op":"set","name":"missing-theme","showError":true},{"op":"emit","theme":"light"},{"op":"instance"},{"op":"disable"}]}),
        json!({"setting":"dark","steps":[{"op":"apply"},{"op":"preview","name":"light"},{"op":"set","name":"light"},{"op":"set","name":"missing-theme"}]}),
        json!({"setting":null,"scheme":null,"background":{"r":255,"g":255,"b":255},"steps":[{"op":"apply"},{"op":"apply"}]}),
        json!({"setting":"light/dark","scheme":null,"background":null,"steps":[{"op":"apply"},{"op":"setting","name":"dark"},{"op":"emit","theme":"light"},{"op":"disable"}]}),
    ];
    for scenario in scenarios {
        let request = json!({"id":1,"module":"modes/interactive/theme/theme-controller.ts","operation":"themeController","setting":scenario["setting"],"scheme":scenario["scheme"],"background":scenario["background"],"steps":scenario["steps"]});
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        let mut initial = JsObject::new();
        if let Some(setting) = scenario["setting"].as_str() {
            initial.insert("theme".into(), pi_js_compat::js_json::JsValue::String(setting.into()));
        }
        let settings = Arc::new(Mutex::new(SettingsManager::in_memory(
            initial,
            SettingsManagerCreateOptions { project_trusted: None },
        )));
        let events = Rc::new(RefCell::new(Vec::new()));
        let ui = Rc::new(Ui {
            events: events.clone(),
            listener: RefCell::new(None),
            scheme: scenario["scheme"].as_str().map(parse_theme),
            background: scenario["background"].as_object().map(|rgb| RgbColor {
                r: rgb["r"].as_f64().unwrap(),
                g: rgb["g"].as_f64().unwrap(),
                b: rgb["b"].as_f64().unwrap(),
            }),
        });
        let error_events = events.clone();
        let changed_events = events.clone();
        let mut controller = InteractiveThemeController::new(
            ui.clone(),
            settings.clone(),
            Rc::new(move |error| error_events.borrow_mut().push(json!(["error", error]))),
            Rc::new(move || changed_events.borrow_mut().push(json!(["changed"]))),
        )
        .unwrap();
        let mut states = Vec::new();
        for step in scenario["steps"].as_array().unwrap() {
            match step["op"].as_str().unwrap() {
                "apply" => controller.apply_from_settings(),
                "set" => {
                    controller.set_theme_name(
                        &JsString::from(step["name"].as_str().unwrap()),
                        step["showError"] == true,
                    );
                }
                "preview" => controller.preview(&JsString::from(step["name"].as_str().unwrap())),
                "instance" => {
                    controller.set_theme_instance(get_theme_by_name(&"light".into()).unwrap());
                }
                "disable" => controller.disable_auto_sync(),
                "setting" => {
                    lock_settings_manager(&settings).set_theme(&JsString::from(step["name"].as_str().unwrap()))
                }
                "emit" => (ui.listener.borrow().as_ref().unwrap())(parse_theme(step["theme"].as_str().unwrap())),
                op => panic!("unknown step {op}"),
            }
            states.push(json!({"active":current_theme().name,"terminal":controller.get_terminal_theme().as_str(),"setting":lock_settings_manager(&settings).get_theme_setting()}));
        }
        assert_eq!(
            json!({"events":*events.borrow(),"states":states}),
            expected["output"],
            "{scenario}"
        );
    }
}
fn parse_theme(theme: &str) -> TerminalTheme {
    match theme {
        "light" => TerminalTheme::Light,
        "dark" => TerminalTheme::Dark,
        _ => panic!("invalid theme {theme}"),
    }
}
