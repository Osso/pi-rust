//! Language names from the exact atlas snapshot.
use pi_js_compat::js_json::JsString;

pub fn language_from_path(path: &JsString) -> JsString {
    let basename = basename(path);
    let extension = basename.split('.').last().unwrap_or_default().to_lowercase();
    let names: &[(&str, &str)] = &[
        ("astro", "Astro"),
        ("bash", "Shell"),
        ("c", "C"),
        ("cc", "C++"),
        ("cjs", "JavaScript"),
        ("clj", "Clojure"),
        ("cljs", "Clojure"),
        ("coffee", "CoffeeScript"),
        ("cpp", "C++"),
        ("cr", "Crystal"),
        ("cs", "C#"),
        ("css", "CSS"),
        ("cts", "TypeScript"),
        ("dart", "Dart"),
        ("dockerfile", "Dockerfile"),
        ("ejs", "EJS"),
        ("elm", "Elm"),
        ("env", "Env"),
        ("erl", "Erlang"),
        ("ex", "Elixir"),
        ("exs", "Elixir"),
        ("fs", "F#"),
        ("fsi", "F#"),
        ("fsx", "F#"),
        ("gitignore", "Gitignore"),
        ("gleam", "Gleam"),
        ("go", "Go"),
        ("gql", "GraphQL"),
        ("graphql", "GraphQL"),
        ("graphqls", "GraphQL"),
        ("groovy", "Groovy"),
        ("h", "C"),
        ("hbs", "Handlebars"),
        ("hpp", "C++"),
        ("hs", "Haskell"),
        ("htm", "HTML"),
        ("html", "HTML"),
        ("html.j2", "Jinja"),
        ("j2", "Jinja"),
        ("java", "Java"),
        ("jinja", "Jinja"),
        ("jinja2", "Jinja"),
        ("jl", "Julia"),
        ("js", "JavaScript"),
        ("json", "JSON"),
        ("jsx", "JavaScript"),
        ("kt", "Kotlin"),
        ("kts", "Kotlin"),
        ("less", "Less"),
        ("liquid", "Liquid"),
        ("lua", "Lua"),
        ("m", "Objective-C"),
        ("md", "Markdown"),
        ("mdx", "Markdown"),
        ("mjs", "JavaScript"),
        ("mm", "Objective-C"),
        ("mts", "TypeScript"),
        ("mustache", "Mustache"),
        ("nix", "Nix"),
        ("njk", "Nunjucks"),
        ("php", "PHP"),
        ("phtml", "PHP"),
        ("pl", "Perl"),
        ("pm", "Perl"),
        ("prisma", "Prisma"),
        ("proto", "Protobuf"),
        ("ps1", "PowerShell"),
        ("psd1", "PowerShell"),
        ("psm1", "PowerShell"),
        ("pug", "Pug"),
        ("py", "Python"),
        ("pyi", "Python"),
        ("qml", "QML"),
        ("r", "R"),
        ("raku", "Raku"),
        ("rb", "Ruby"),
        ("rs", "Rust"),
        ("scala", "Scala"),
        ("scss", "SCSS"),
        ("sh", "Shell"),
        ("sol", "Solidity"),
        ("sql", "SQL"),
        ("styl", "Stylus"),
        ("svelte", "Svelte"),
        ("swift", "Swift"),
        ("tf", "Terraform"),
        ("toml", "TOML"),
        ("ts", "TypeScript"),
        ("tsx", "TypeScript"),
        ("twig", "Twig"),
        ("vue", "Vue"),
        ("wasm", "WebAssembly"),
        ("wat", "WebAssembly"),
        ("xml", "XML"),
        ("yaml", "YAML"),
        ("yml", "YAML"),
        ("zig", "Zig"),
        ("zsh", "Shell"),
    ];
    names
        .iter()
        .find(|(extension_name, _)| extension == *extension_name)
        .map_or_else(|| "Other".into(), |(_, name)| (*name).into())
}

pub fn basename(path: &JsString) -> JsString {
    path.split('/')
        .filter(|part| !part.is_empty())
        .last()
        .unwrap_or_default()
}

pub fn format_cost(value: f64) -> JsString {
    use pi_js_compat::number::math_round;
    let (amount, suffix, precision) = if value >= 1_000_000.0 {
        (value / 1_000_000.0, "M", 2)
    } else if value >= 1_000.0 {
        (value / 1_000.0, "k", 2)
    } else if value > 0.0 && value < 0.01 {
        (value, "", 4)
    } else {
        let fudge = if value >= 1.0 { 0.1 } else { 0.0 };
        (math_round(value * 100.0 + fudge) / 100.0, "", 2)
    };
    let formatted = intl_decimal(amount.abs(), precision);
    format!(
        "{}${formatted}{suffix}",
        if amount.is_sign_negative() { "-" } else { "" }
    )
    .into()
}

pub fn format_number(value: f64, round: bool) -> JsString {
    let (value, suffix) = if value >= 1_000_000_000.0 {
        (value / 1_000_000_000.0, "B")
    } else if value >= 1_000_000.0 {
        (value / 1_000_000.0, "M")
    } else if value >= 1_000.0 {
        (value / 1_000.0, "k")
    } else {
        (value, "")
    };
    let value = if round {
        pi_js_compat::number::math_round(value)
    } else {
        value
    };
    format!("{}{suffix}", intl_decimal(value, 2)).into()
}

/// Intl rounds the shortest decimal representation, unlike binary toFixed.
fn intl_decimal(value: f64, precision: usize) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN"
        } else if value.is_sign_negative() {
            "-∞"
        } else {
            "∞"
        }
        .into();
    }
    let decimal = pi_js_compat::number::js_number_to_string(value.abs());
    let (mantissa, exponent) = decimal
        .split_once('e')
        .map_or((decimal.as_str(), 0), |(mantissa, exponent)| {
            (mantissa, exponent.parse::<i32>().expect("decimal exponent"))
        });
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let digits = mantissa.replace('.', "");
    let (integer, fraction) = if point <= 0 {
        ("0".into(), "0".repeat((-point) as usize) + &digits)
    } else if point as usize >= digits.len() {
        (
            digits.clone() + &"0".repeat(point as usize - digits.len()),
            String::new(),
        )
    } else {
        (digits[..point as usize].into(), digits[point as usize..].into())
    };
    let (integer, fraction) = round_decimal(integer, fraction, precision);
    let mut grouped = String::new();
    if value.is_sign_negative() {
        grouped.push('-');
    }
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if !fraction.is_empty() {
        grouped.push('.');
        grouped.push_str(&fraction);
    }
    grouped
}

fn round_decimal(integer: String, mut fraction: String, precision: usize) -> (String, String) {
    if fraction.len() > precision {
        let round_up = fraction.as_bytes()[precision] >= b'5';
        fraction.truncate(precision);
        let mut digits = (integer + &fraction).into_bytes();
        if round_up {
            let mut carry = true;
            for digit in digits.iter_mut().rev() {
                if *digit < b'9' {
                    *digit += 1;
                    carry = false;
                    break;
                }
                *digit = b'0';
            }
            if carry {
                digits.insert(0, b'1');
            }
        }
        let fraction = digits.split_off(digits.len() - precision);
        return (
            String::from_utf8(digits).expect("decimal digits"),
            String::from_utf8(fraction)
                .expect("decimal digits")
                .trim_end_matches('0')
                .into(),
        );
    }
    (integer, fraction.trim_end_matches('0').into())
}

pub fn format_model_name(value: &JsString) -> JsString {
    use pi_js_compat::js_regex::JsRegex;
    let suffix = JsRegex::new(r"-\d{8}$|-\d{4}-\d{2}-\d{2}$").expect("constant model suffix regex");
    let name = suffix
        .replace_all(value.clone(), "")
        .replace("-", " ")
        .replace("_", " ");
    let mut previous_word = false;
    JsString::from_utf16(
        name.units()
            .iter()
            .map(|unit| {
                let word = matches!(unit, 48..=57 | 65..=90 | 97..=122 | 95);
                let output = if word && !previous_word && matches!(unit, 97..=122) {
                    *unit - 32
                } else {
                    *unit
                };
                previous_word = word;
                output
            })
            .collect(),
    )
}

pub fn format_cache_timestamp(iso: &JsString, now: f64, zone: &jiff::tz::TimeZone) -> JsString {
    use pi_js_compat::date::{date_parse_utf16, local_date_fields_in};
    let Some(date) = local_date_fields_in(date_parse_utf16(iso.units()) as i64, zone) else {
        return "undefined NaN, NaN".into();
    };
    let Some(today) = local_date_fields_in(now as i64, zone) else {
        return "undefined NaN, NaN".into();
    };
    let yesterday = local_date_fields_in((now - 86_400_000.0) as i64, zone);
    let hour = if date.hours % 12 == 0 { 12 } else { date.hours % 12 };
    let time = format!(
        "{hour}:{:02} {}",
        date.minutes,
        if date.hours >= 12 { "PM" } else { "AM" }
    );
    let same_day = |other: pi_js_compat::date::LocalDateFields| {
        (date.year, date.month, date.day) == (other.year, other.month, other.day)
    };
    if same_day(today) {
        return time.into();
    }
    if yesterday.is_some_and(same_day) {
        return format!("Yesterday {time}").into();
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][date.month as usize];
    if date.year == today.year {
        format!("{month} {}, {time}", date.day).into()
    } else {
        format!("{month} {}, {}", date.day, date.year).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn unmapped_extensions_are_other_like_source_ext_to_lang() {
        // pi-atlas src/format.ts:109-112: EXT_TO_LANG has no style/currency keys.
        assert_eq!(language_from_path(&"theme.style".into()), "Other");
        assert_eq!(language_from_path(&"prices.currency".into()), "Other");
        assert_eq!(language_from_path(&"main.rs".into()), "Rust");
    }
    #[test]
    fn formats_timestamp_relative_to_local_calendar() {
        let now = pi_js_compat::date::date_parse("2026-10-25T12:00:00Z");
        for (iso, expected) in [
            ("2026-10-25T10:05:00Z", "10:05 AM"),
            ("2026-10-24T19:07:00Z", "Yesterday 7:07 PM"),
            ("2026-10-01T00:00:00Z", "Oct 1, 12:00 AM"),
            ("2025-10-25T12:00:00Z", "Oct 25, 2025"),
        ] {
            assert_eq!(
                format_cache_timestamp(&iso.into(), now, &jiff::tz::TimeZone::UTC),
                expected
            );
        }
    }
    #[test]
    fn matches_frozen_number_currency_and_model_names() -> Result<(), JsString> {
        let fixtures =
            js_json::parse_lossless(include_str!("fixtures/domain.json")).map_err(|error| error.into_message())?;
        for case in fixtures["formatting"]["cost"].as_array().expect("cost") {
            assert_eq!(
                format_cost(case["value"].as_f64().expect("number")),
                *case["expected"].as_string().expect("text")
            );
        }
        for case in fixtures["formatting"]["number"].as_array().expect("number") {
            assert_eq!(
                format_number(
                    case["value"].as_f64().expect("number"),
                    case["round"].as_bool().expect("bool")
                ),
                *case["expected"].as_string().expect("text")
            );
        }
        for case in fixtures["formatting"]["model"].as_array().expect("model") {
            assert_eq!(
                format_model_name(case["value"].as_string().expect("text")),
                *case["expected"].as_string().expect("text")
            );
        }
        Ok(())
    }
}
