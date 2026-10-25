//! Ported from `coding-agent/src/modes/interactive/components/earendil-announcement.ts`.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::OnceLock;

use pi_js_compat::{base64::buffer_to_base64, js_json::JsString};
use pi_tui::components::image::{ImageOptions, ImageTheme};
use pi_tui::tui::component;
use pi_tui::{Component, Container, DynamicBorder, Image, Spacer, Text};

use crate::config::get_bundled_interactive_asset_path;

use crate::modes::interactive::theme::{Theme, ThemeColor};

const BLOG_URL: &str = "https://mariozechner.at/posts/2026-04-08-ive-sold-out/";
const IMAGE_FILENAME: &str = "clankolas.png";

static CACHED_IMAGE_BASE64: OnceLock<Option<String>> = OnceLock::new();

fn load_image_base64() -> Option<String> {
    CACHED_IMAGE_BASE64
        .get_or_init(|| {
            get_bundled_interactive_asset_path(IMAGE_FILENAME)
                .ok()
                .and_then(|path| std::fs::read(pi_js_compat::js_json::encode_node_utf8(&path)).ok())
                .map(|bytes| buffer_to_base64(&bytes))
        })
        .clone()
}

/// Announcement panel shown once after the rename to Earendil.
pub struct EarendilAnnouncementComponent {
    container: Container,
}

impl EarendilAnnouncementComponent {
    pub fn new(theme: Arc<Theme>) -> Self {
        let mut container = Container::new();
        let accent_border = {
            let theme = theme.clone();
            Rc::new(move |text: &JsString| theme.fg(ThemeColor::Accent, text))
        };
        container.add_child(component(DynamicBorder::new(accent_border.clone())));
        container.add_child(component(Text::new(
            theme.bold(theme.fg(ThemeColor::Accent, "pi has joined Earendil")),
            1,
            0,
        )));
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(Text::new(
            theme.fg(ThemeColor::Muted, "Read the blog post:"),
            1,
            0,
        )));
        container.add_child(component(Text::new(theme.fg(ThemeColor::MdLink, BLOG_URL), 1, 0)));
        container.add_child(component(Spacer::new(1)));

        if let Some(image_base64) = load_image_base64() {
            let fallback_theme = theme.clone();
            let image_theme = Rc::new(ImageTheme {
                fallback_color: Rc::new(move |text: &JsString| fallback_theme.fg(ThemeColor::Muted, text)),
            });
            container.add_child(component(Image::new(
                image_base64,
                "image/png",
                image_theme,
                ImageOptions {
                    max_width_cells: Some(56.0),
                    filename: Some(IMAGE_FILENAME.into()),
                    ..Default::default()
                },
                None,
            )));
            container.add_child(component(Spacer::new(1)));
        }

        container.add_child(component(DynamicBorder::new(accent_border)));
        Self { container }
    }
}

impl Component for EarendilAnnouncementComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render_children(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.container.invalidate_children();
    }
}
