use std::rc::Rc;

use pi_tui::terminal_image::{ImageDimensions, TerminalCapabilities, set_capabilities};
use pi_tui::{Component, Image, ImageOptions, ImageTheme, Text};

#[test]
fn text_padding_mutation_reflows_and_invalidates_cached_output() {
    let mut text = Text::new("hello world", 0, 0);
    assert_eq!(text.render(12), ["hello world "]);
    text.set_padding(1, 1);
    assert_eq!(
        text.render(12),
        ["            ", " hello      ", " world      ", "            "]
    );
    text.set_padding(1, 1);
    assert_eq!(text.render(12).len(), 4);
}

#[test]
fn image_fallback_is_not_truncated_or_hyperlinked_by_the_component() {
    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: true,
    });
    let mut image = Image::new(
        "AAAA",
        "image/png",
        Rc::new(ImageTheme {
            fallback_color: Rc::new(|text| format!("<{text}>")),
        }),
        ImageOptions {
            filename: Some("/tmp/image.png".into()),
            ..ImageOptions::default()
        },
        Some(ImageDimensions {
            width_px: 10,
            height_px: 20,
        }),
    );
    assert_eq!(image.render(4), ["<[Image: /tmp/image.png [image/png] 10x20]>"]);
}
