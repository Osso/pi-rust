use crate::shared_oracle as oracle;
use pi_coding_agent::{
    modes::interactive::theme::{self, ColorMode, ColorValue, REQUIRED_COLORS, Theme, ThemeBg, ThemeColor},
    utils::uncaught::{catch_panics, format_uncaught},
};
use pi_js_compat::{
    js_format, js_json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
    process_env,
};
use std::{rc::Rc, sync::Arc};

fn number(value: &JsValue) -> f64 {
    match value {
        JsValue::Number(n) => *n,
        JsValue::String(s) => pi_js_compat::number::parse_number(&s.to_utf8().unwrap()),
        _ => panic!(),
    }
}
fn direct(value: &JsValue, name: Option<&str>, mode: ColorMode) -> Result<Theme, theme::ThemeError> {
    let color = match value {
        JsValue::String(s) if !["NaN", "Infinity", "-Infinity"].iter().any(|sentinel| s == *sentinel) => {
            ColorValue::Text(s.clone())
        }
        _ => ColorValue::Index(number(value)),
    };
    let fg: Vec<_> = REQUIRED_COLORS
        .iter()
        .map(|key| (JsString::from(*key), color.clone()))
        .collect();
    let bg = [("selectedBg".into(), color)];
    Theme::new(&fg, &bg, mode, name.map(Into::into), None)
}
fn capture(run: impl FnOnce() -> Result<JsValue, theme::ThemeError>) -> JsValue {
    match catch_panics(run) {
        Ok(Ok(value)) => js_json!({"ok":true,"value":value}),
        Ok(Err(error)) => js_json!({"ok":false,"error":js_format!("Error: {}",error.message())}),
        Err(error) => js_json!({"ok":false,"error":format_uncaught(error.as_ref())}),
    }
}
fn adapter_snapshot(mode: ColorMode) -> JsValue {
    theme::set_theme_instance(Arc::new(direct(&JsValue::Number(12.0), None, mode).unwrap()));
    let markdown = theme::get_markdown_theme();
    let select = theme::get_select_list_theme();
    let editor = theme::get_editor_theme();
    let settings = theme::get_settings_list_theme();
    theme::set_theme_instance(Arc::new(direct(&JsValue::Number(13.0), None, mode).unwrap()));
    let text = JsString::from_utf16(vec![65, 0xd800]);
    js_json!({
        "markdown":[markdown.heading(&text),markdown.link(&text),markdown.link_url(&text),markdown.code(&text),markdown.code_block(&text),markdown.code_block_border(&text),markdown.quote(&text),markdown.quote_border(&text),markdown.hr(&text),markdown.list_bullet(&text),markdown.bold(&text),markdown.italic(&text),markdown.underline(&text),markdown.strikethrough(&text)],
        "select":[(select.selected_prefix)(&text),(select.selected_text)(&text),(select.description)(&text),(select.scroll_info)(&text),(select.no_match)(&text)],
        "editor":[(editor.border_color)(&text),(editor.prompt_prefix.unwrap())(&text)],
        "settings":[(settings.label)(&text,false),(settings.label)(&text,true),(settings.value)(&text,false),(settings.value)(&text,true),(settings.description)(&text),settings.cursor,(settings.hint)(&text)]
    })
}

#[test]
fn theme_constructor_numbers_registration_and_error_boundaries_match_frozen() {
    process_env::set("FORCE_COLOR", "1");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "theme-api-oracle.mts",
        "packages/coding-agent/src/modes/interactive/theme/theme.ts",
        &[("FORCE_COLOR", "1".into())],
    );
    let values = js_json!([
        0,
        -0.0,
        1.5,
        -2.75,
        1e30,
        "NaN",
        "Infinity",
        "-Infinity",
        "#-1ff00",
        "#+fff00",
        "# 1ff00",
        "#f_00ff",
        "",
        "not-a-color",
        "#bad"
    ]);
    for (mode, mode_name) in [(ColorMode::Truecolor, "truecolor"), (ColorMode::Color256, "256color")] {
        let mut output = vec![capture(|| {
            Ok(JsValue::String(
                theme::get_markdown_theme().strikethrough(&JsString::from_utf16(vec![115, 0xd800])),
            ))
        })];
        for value in values.as_array().unwrap() {
            output.push(capture(|| {
                let theme = Arc::new(direct(value,None,mode)?);
                let text = JsString::from_utf16(vec![99,0xd800]);
                Ok(js_json!({
                    "fg":theme.fg(ThemeColor::Accent,JsString::from_utf16(vec![97,0xd800])),
                    "bg":theme.bg(ThemeBg::SelectedBg,JsString::from_utf16(vec![98,0xdc00])),
                    "ansi":[theme.get_fg_ansi(ThemeColor::Accent),theme.get_bg_ansi(ThemeBg::SelectedBg)],
                    "bash":(theme.get_bash_mode_border_color())(&text),
                    "styles":[theme.bold(&text),theme.italic(&text),theme.underline(&text),theme.inverse(&text),theme.strikethrough(&text)]
                }))
            }));
        }
        theme::set_registered_themes(vec![Arc::new(
            direct(&JsValue::Number(1.0), Some("old"), mode).unwrap(),
        )])
        .unwrap();
        output.push(capture(|| {
            theme::set_registered_themes(
                [("first", 2.0), ("bad/name", 3.0), ("last", 4.0)]
                    .into_iter()
                    .map(|(name, index)| Arc::new(direct(&JsValue::Number(index), Some(name), mode).unwrap()))
                    .collect(),
            )?;
            Ok(JsValue::Null)
        }));
        output.push(JsValue::Array(
            ["old", "first", "last"]
                .iter()
                .map(|name| {
                    theme::get_theme_by_name(&(*name).into()).map_or(JsValue::Null, |theme| {
                        JsValue::String(theme.get_fg_ansi(ThemeColor::Accent).into())
                    })
                })
                .collect(),
        ));
        theme::on_theme_change(Rc::new(|| {
            std::panic::panic_any(NativeNodeError::from_constructor(
                NativeErrorClass::TypeError,
                JsString::from_utf16(vec![99, 97, 108, 108, 98, 97, 99, 107, 0xd800]),
            ))
        }));
        output.push(capture(|| {
            let result = theme::set_theme(&"first".into(), false)?;
            Ok(js_json!({"success":result.success,"error":result.error}))
        }));
        output.push(js_json!({"accent":theme::current_theme().get_fg_ansi(ThemeColor::Accent),"name":theme::current_theme().name}));
        theme::on_theme_change(Rc::new(|| {}));
        let sparse = Theme::new(
            &[("mdCodeBlock".into(), ColorValue::Index(7.0))],
            &[],
            ColorMode::Color256,
            None,
            None,
        )
        .unwrap();
        theme::set_theme_instance(Arc::new(sparse));
        output.push(capture(|| {
            Ok(JsValue::Array(
                theme::highlight_code(&"const x = 1;".into(), Some(&"javascript".into()))
                    .unwrap()
                    .into_iter()
                    .map(JsValue::String)
                    .collect(),
            ))
        }));
        output.push(capture(|| {
            Ok(JsValue::Array(
                theme::get_markdown_theme()
                    .highlight_code(&"const x = 1;".into(), Some(&"javascript".into()))
                    .unwrap()
                    .into_iter()
                    .map(JsValue::String)
                    .collect(),
            ))
        }));
        theme::set_registered_themes(
            [("", 9.0), ("eq", 1.0), ("eq", 2.0), ("Z", 3.0), ("é", 4.0), ("é", 5.0)]
                .into_iter()
                .map(|(name, index)| Arc::new(direct(&JsValue::Number(index), Some(name), mode).unwrap()))
                .collect(),
        )
        .unwrap();
        output.push(js_json!({"names":theme::get_available_themes().unwrap(),"duplicate":theme::get_theme_by_name(&"eq".into()).unwrap().get_fg_ansi(ThemeColor::Accent)}));
        output.push(adapter_snapshot(mode));
        let expected: JsValue = source.request(
            &js_json!({"id":1,"colors":REQUIRED_COLORS,"backgrounds":["selectedBg"],"mode":mode_name,"values":values}),
        );
        assert_eq!(JsValue::Array(output), expected["output"], "mode {mode_name}");
    }
    process_env::clear();
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
