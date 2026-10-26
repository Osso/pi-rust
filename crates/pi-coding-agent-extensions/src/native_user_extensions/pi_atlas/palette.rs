//! Only the atlas language/provider color mappings are retained.
use crate::utils::chalk::{Chalk, Level, Style};
use pi_js_compat::js_json::JsString;
const LANGUAGES: &[(&str, u32)] = &[
    ("Astro", 0xFF5A03),
    ("C", 0x555555),
    ("C#", 0x178600),
    ("C++", 0xF34B7D),
    ("Clojure", 0xDB5855),
    ("CoffeeScript", 0x244776),
    ("Crystal", 0x000100),
    ("CSS", 0x663399),
    ("Dart", 0x00B4AB),
    ("Dockerfile", 0x384D54),
    ("EJS", 0xA91E50),
    ("Elixir", 0x6E4A7E),
    ("Elm", 0x60B5CC),
    ("Env", 0xE5D559),
    ("Erlang", 0xB83998),
    ("F#", 0xB845FC),
    ("Gitignore", 0xF44D27),
    ("Gleam", 0xFFAFF3),
    ("Go", 0x00ADD8),
    ("GraphQL", 0xE10098),
    ("Groovy", 0x4298B8),
    ("HTML", 0xE34C26),
    ("Handlebars", 0xF7931E),
    ("Haskell", 0x5E5086),
    ("Java", 0xB07219),
    ("JavaScript", 0xF1E05A),
    ("Jinja", 0xA52A22),
    ("JSON", 0x292929),
    ("Julia", 0xA270BA),
    ("Kotlin", 0xA97BFF),
    ("Less", 0x1D365D),
    ("Liquid", 0x67B8DE),
    ("Lua", 0x000080),
    ("Markdown", 0x083FA1),
    ("Mustache", 0x724B3B),
    ("Nix", 0x7E7EFF),
    ("Nunjucks", 0x3D8137),
    ("Objective-C", 0x438EFF),
    ("PHP", 0x4F5D95),
    ("Perl", 0x0298C3),
    ("PowerShell", 0x012456),
    ("Prisma", 0x0C344B),
    ("Protobuf", 0x652D90),
    ("Pug", 0xA86454),
    ("Python", 0x3572A5),
    ("QML", 0x44A51C),
    ("R", 0x198CE7),
    ("Raku", 0x0000FB),
    ("Ruby", 0x701516),
    ("Rust", 0xDEA584),
    ("SCSS", 0xC6538C),
    ("SQL", 0xE38C00),
    ("Scala", 0xC22D40),
    ("Shell", 0x89E051),
    ("Solidity", 0xAA6746),
    ("Svelte", 0xFF3E00),
    ("Stylus", 0xFF6347),
    ("Swift", 0xF05138),
    ("TOML", 0x9C4221),
    ("Terraform", 0x844FBA),
    ("TypeScript", 0x3178C6),
    ("Twig", 0xC1D026),
    ("Vue", 0x41B883),
    ("Visual Basic .NET", 0x945DB7),
    ("WebAssembly", 0x04133B),
    ("XML", 0x0060AC),
    ("YAML", 0xCB171E),
    ("Zig", 0xEC915C),
];
const PROVIDERS: &[(&str, u32)] = &[
    ("amazon-bedrock", 0xFF9900),
    ("ant-ling", 0x0d66fd),
    ("anthropic", 0xd66545),
    ("azure-openai-responses", 0x0078D4),
    ("cerebras", 0x7CFF00),
    ("cloudflare-ai-gateway", 0xF38020),
    ("cloudflare-workers-ai", 0xF38020),
    ("deepseek", 0x4D6BFE),
    ("fireworks", 0xFD6F3E),
    ("github-copilot", 0x93f5eb),
    ("google", 0x4285F4),
    ("google-vertex", 0x4285F4),
    ("groq", 0xF55036),
    ("huggingface", 0xFFD21E),
    ("kimi-coding", 0x1F6BFF),
    ("minimax", 0x5B6CFF),
    ("minimax-cn", 0x5B6CFF),
    ("mistral", 0xFA520F),
    ("moonshotai", 0x4F46E5),
    ("moonshotai-cn", 0x4F46E5),
    ("nvidia", 0x76B900),
    ("openai", 0x0f9f7c),
    ("openai-codex", 0x009aa2),
    ("opencode", 0xd0cfce),
    ("opencode-go", 0xd0cfce),
    ("openrouter", 0x7C3AED),
    ("together", 0x0F6FFF),
    ("vercel-ai-gateway", 0xd1caf3),
    ("xai", 0xffffff),
    ("xiaomi", 0xFF6900),
    ("xiaomi-token-plan-cn", 0xFF6900),
    ("xiaomi-token-plan-ams", 0xFF6900),
    ("xiaomi-token-plan-sgp", 0xFF6900),
    ("zai", 0x2563EB),
    ("zai-coding-cn", 0x2563EB),
];

pub fn paint(name: &JsString, provider: bool, text: JsString) -> JsString {
    paint_at_level(name, provider, text, Chalk::stdout())
}

fn paint_at_level(name: &JsString, provider: bool, text: JsString, chalk: Chalk) -> JsString {
    let mapping = if provider { PROVIDERS } else { LANGUAGES };
    let Some((_, rgb)) = mapping.iter().find(|(candidate, _)| name == *candidate) else {
        return chalk.paint(&[Style::White], text);
    };
    if chalk.level() == Level::Plain || text.is_empty() {
        return text;
    }
    let red = (rgb >> 16) & 255;
    let green = (rgb >> 8) & 255;
    let blue = rgb & 255;
    let open = match chalk.level() {
        Level::Ansi16m => format!("\x1b[38;2;{red};{green};{blue}m"),
        Level::Ansi256 => format!("\x1b[38;5;{}m", rgb_to_256(red, green, blue)),
        Level::Basic => format!("\x1b[{}m", rgb_to_ansi(red, green, blue)),
        Level::Plain => unreachable!("plain level returned above"),
    };
    let text = text.replace("\x1b[39m", open.clone());
    JsString::from(open) + text + "\x1b[39m"
}

fn rgb_to_256(red: u32, green: u32, blue: u32) -> u32 {
    if red == green && green == blue {
        if red < 8 {
            return 16;
        }
        if red > 248 {
            return 231;
        }
        return (((red as f64 - 8.0) / 247.0 * 24.0).round() as u32) + 232;
    }
    16 + 36 * (red as f64 / 255.0 * 5.0).round() as u32
        + 6 * (green as f64 / 255.0 * 5.0).round() as u32
        + (blue as f64 / 255.0 * 5.0).round() as u32
}
fn rgb_to_ansi(red: u32, green: u32, blue: u32) -> u32 {
    let code = rgb_to_256(red, green, blue);
    let (red, green, blue) = if code >= 232 {
        let value = f64::from((code - 232) * 10 + 8) / 255.0;
        (value, value, value)
    } else {
        let code = code - 16;
        (
            f64::from(code / 36) / 5.0,
            f64::from((code % 36) / 6) / 5.0,
            f64::from(code % 6) / 5.0,
        )
    };
    let value = red.max(green).max(blue) * 2.0;
    if value == 0.0 {
        return 30;
    }
    let bits = ((blue.round() as u32) << 2) | ((green.round() as u32) << 1) | red.round() as u32;
    30 + bits + if value == 2.0 { 60 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn all_palette_entries_match_chalk_at_every_color_level() -> Result<(), JsString> {
        let fixture =
            js_json::parse_lossless(include_str!("fixtures/palette.json")).map_err(|error| error.into_message())?;
        for case in fixture.as_array().expect("palette cases") {
            let level = Level::from_number(case["level"].as_u64().expect("level") as u8).expect("valid level");
            let actual = paint_at_level(
                case["name"].as_string().expect("name"),
                case["provider"].as_bool().expect("provider"),
                "■■".into(),
                Chalk::new(level),
            );
            assert_eq!(actual, *case["expected"].as_string().expect("expected"), "{:?}", case);
        }
        Ok(())
    }
}
