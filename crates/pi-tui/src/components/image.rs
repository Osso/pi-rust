//! Ported from `packages/tui/src/components/image.ts`.

use pi_js_compat::{
    js_json::JsString,
    number::{js_math_max, js_math_min, js_number_to_string},
};
use std::rc::Rc;

use super::text::StyleFn;
use crate::terminal_image::{
    ImageDimensions, ImageRenderOptions, allocate_image_id, get_capabilities, get_cell_dimensions,
    get_image_dimensions, image_fallback, render_image,
};
use crate::tui::Component;

const DEFAULT_MAX_WIDTH_CELLS: f64 = 60.0;

pub struct ImageTheme {
    pub fallback_color: StyleFn,
}

#[derive(Clone, Default)]
pub struct ImageOptions {
    pub max_width_cells: Option<f64>,
    pub max_height_cells: Option<f64>,
    pub filename: Option<JsString>,
    /// Kitty image ID. If provided, reuses this ID (for animations/updates).
    pub image_id: Option<u32>,
}

pub struct Image {
    base64_data: JsString,
    mime_type: JsString,
    dimensions: ImageDimensions,
    theme: Rc<ImageTheme>,
    options: ImageOptions,
    image_id: Option<u32>,
    cached_lines: Option<Vec<JsString>>,
    cached_width: Option<usize>,
}

impl Image {
    pub fn new(
        base64_data: impl Into<JsString>,
        mime_type: impl Into<JsString>,
        theme: Rc<ImageTheme>,
        options: ImageOptions,
        dimensions: Option<ImageDimensions>,
    ) -> Self {
        let base64_data = base64_data.into();
        let mime_type = mime_type.into();
        let dimensions = dimensions
            .or_else(|| get_image_dimensions(&base64_data, &mime_type))
            .unwrap_or(ImageDimensions {
                width_px: 800,
                height_px: 600,
            });
        Self {
            image_id: options.image_id,
            base64_data,
            mime_type,
            dimensions,
            theme,
            options,
            cached_lines: None,
            cached_width: None,
        }
    }

    pub fn image_id(&self) -> Option<u32> {
        self.image_id
    }

    fn fallback_line(&self) -> JsString {
        let fallback = image_fallback(&self.mime_type, Some(self.dimensions), self.options.filename.as_ref());
        (self.theme.fallback_color)(&fallback)
    }
}

impl Component for Image {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        if let (Some(lines), Some(cached_width)) = (&self.cached_lines, self.cached_width)
            && cached_width == width
        {
            return lines.clone();
        }

        let max_width = js_math_max(
            1.0,
            js_math_min(
                width as f64 - 2.0,
                self.options.max_width_cells.unwrap_or(DEFAULT_MAX_WIDTH_CELLS),
            ),
        );
        let cell_dimensions = get_cell_dimensions();
        let default_max_height = js_math_max(
            1.0,
            ((max_width * cell_dimensions.width_px as f64) / cell_dimensions.height_px as f64).ceil(),
        );
        let max_height = self.options.max_height_cells.unwrap_or(default_max_height);

        let capabilities = get_capabilities();
        let lines = match capabilities.images {
            Some(protocol) => {
                if protocol == crate::terminal_image::ImageProtocol::Kitty && self.image_id.is_none() {
                    self.image_id = Some(allocate_image_id());
                }
                let rendered = render_image(
                    &self.base64_data,
                    self.dimensions,
                    &ImageRenderOptions {
                        max_width_cells: Some(max_width),
                        max_height_cells: Some(max_height),
                        preserve_aspect_ratio: None,
                        image_id: self.image_id,
                        move_cursor: Some(false),
                    },
                );
                match rendered {
                    Some(result) => {
                        if result.image_id.is_some_and(|id| id != 0) {
                            self.image_id = result.image_id;
                        }
                        if protocol == crate::terminal_image::ImageProtocol::Kitty {
                            let mut lines = vec![result.sequence];
                            for _ in 0..(result.rows - 1.0).ceil() as usize {
                                lines.push(JsString::default());
                            }
                            lines
                        } else {
                            let mut lines = Vec::new();
                            for _ in 0..(result.rows - 1.0).ceil() as usize {
                                lines.push(JsString::default());
                            }
                            let row_offset = result.rows - 1.0;
                            let move_up = if row_offset > 0.0 {
                                JsString::from(format!("\x1b[{}A", js_number_to_string(row_offset)))
                            } else {
                                JsString::default()
                            };
                            lines.push(move_up + result.sequence);
                            lines
                        }
                    }
                    None => vec![self.fallback_line()],
                }
            }
            None => vec![self.fallback_line()],
        };

        self.cached_lines = Some(lines.clone());
        self.cached_width = Some(width);
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.cached_lines = None;
        self.cached_width = None;
    }
}
