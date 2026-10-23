#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::js_json::JsString;
use pi_tui::terminal_image::{
    CellDimensions, ImageDimensions, ImageProtocol, TerminalCapabilities, set_capabilities, set_cell_dimensions,
};
use pi_tui::{Component, Image, ImageOptions, ImageTheme};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn number(value: &Value) -> Option<f64> {
    if value.is_null() {
        None
    } else {
        Some(
            value
                .as_f64()
                .unwrap_or_else(|| value.as_str().unwrap().parse().unwrap()),
        )
    }
}
fn native(request: &Value) -> Value {
    set_capabilities(TerminalCapabilities {
        images: match request["protocol"].as_str() {
            Some("kitty") => Some(ImageProtocol::Kitty),
            Some("iterm2") => Some(ImageProtocol::Iterm2),
            _ => None,
        },
        true_color: true,
        hyperlinks: false,
    });
    set_cell_dimensions(CellDimensions {
        width_px: 9,
        height_px: 18,
    });
    let seen = Rc::new(RefCell::new(Vec::new()));
    let capture = seen.clone();
    let mut image = Image::new(
        "AAAA",
        "image/png",
        Rc::new(ImageTheme {
            fallback_color: Rc::new(move |value| {
                capture.borrow_mut().push(value.clone());
                JsString::from("fallback:") + value
            }),
        }),
        ImageOptions {
            max_width_cells: number(&request["maxWidth"]),
            max_height_cells: number(&request["maxHeight"]),
            image_id: Some(42),
            ..Default::default()
        },
        Some(ImageDimensions {
            width_px: 800,
            height_px: 600,
        }),
    );
    let snapshot = |image: &mut Image, width| json!({"lines":image.render(width).iter().map(|line|line.units().to_vec()).collect::<Vec<_>>(),"imageId":image.image_id(),"fallback":seen.borrow().iter().map(|line|line.units().to_vec()).collect::<Vec<_>>()});
    let width = request["width"].as_u64().unwrap() as usize;
    let mut output = vec![snapshot(&mut image, width), snapshot(&mut image, width)];
    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: false,
    });
    output.push(snapshot(&mut image, width));
    image.invalidate();
    output.push(snapshot(&mut image, width));
    output.push(snapshot(&mut image, width + 1));
    json!(output)
}
#[test]
fn image_fractional_and_nonfinite_options_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn("image-component-numbers-oracle.mts", "packages/tui/src");
    let mut failures = Vec::new();
    let mut count = 0;
    for protocol in [json!(null), json!("kitty"), json!("iterm2")] {
        for width in [0, 1, 2, 6, 80] {
            for maximum in [
                json!(null),
                json!(1.5),
                json!(7.9),
                json!(-3.2),
                json!("NaN"),
                json!("Infinity"),
                json!("-Infinity"),
            ] {
                for height in [json!(null), json!(2.9), json!(0.5), json!("NaN")] {
                    let request = json!({"protocol":protocol,"width":width,"maxWidth":maximum,"maxHeight":height});
                    let expected = oracle.request(&request);
                    assert_eq!(expected["ok"], true, "oracle {request}: {expected}");
                    let actual = native(&request);
                    if actual != expected["output"] {
                        if failures.len() < 3 {
                            eprintln!("{request}: native {actual} / source {}", expected["output"]);
                        }
                        failures.push(request);
                    }
                    count += 1;
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} of {count} image cases differ", failures.len());
}
