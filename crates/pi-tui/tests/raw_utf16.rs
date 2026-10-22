use pi_tui::components::input::{Input, InputOptions};
use pi_tui::utils::extract_ansi_code;

#[test]
fn ansi_offsets_are_native_utf16_units() {
    let ansi = extract_ansi_code("😀\x1b[31m", 2).expect("ESC follows two native UTF-16 units");
    assert_eq!(ansi.code, "\x1b[31m");
    assert_eq!(ansi.length, 5);
}

#[test]
fn input_preserves_cursor_inside_a_replacement_pair() {
    let mut input = Input::new(InputOptions::default());
    input.handle_input("a");
    input.set_value("😀");
    assert_eq!(input.cursor(), 1);
    input.handle_input("x");
    // Frozen JS inserts between the two surrogate units, not after the scalar.
    // JSON assertions become raw-unit assertions once the representation exists.
    assert_eq!(input.cursor(), 2);
}
