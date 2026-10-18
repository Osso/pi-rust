use std::rc::Rc;

use pi_tui::components::dynamic_border::DynamicBorder;
use pi_tui::tui::Component;

#[test]
fn renders_a_full_width_border() {
    let mut border = DynamicBorder::default();
    assert_eq!(border.render(4), vec!["────"]);
}

#[test]
fn applies_a_custom_formatter() {
    let mut border = DynamicBorder::new(Rc::new(|line: &str| format!("[{line}]")));
    assert_eq!(border.render(3), vec!["[───]"]);
}

#[test]
fn renders_at_least_one_cell() {
    let mut border = DynamicBorder::default();
    assert_eq!(border.render(0), vec!["─"]);
}
