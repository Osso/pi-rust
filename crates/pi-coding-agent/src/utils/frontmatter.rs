use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use pi_js_compat::number::js_number_value;
use serde_yaml_ng::Value as YamlValue;

/// Parsed frontmatter and the trimmed body that follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFrontmatter {
    pub frontmatter: JsValue,
    pub body: JsString,
}

/// Frontmatter YAML could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontmatterError {
    message: JsString,
}

impl FrontmatterError {
    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl std::fmt::Display for FrontmatterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for FrontmatterError {}

/// Parse YAML frontmatter delimited by leading and trailing `---` lines.
pub fn parse_frontmatter(content: impl Into<JsString>) -> Result<ParsedFrontmatter, FrontmatterError> {
    let normalized = content.into().replace("\r\n", "\n").replace('\r', "\n");
    let Some((yaml_string, body)) = extract_frontmatter(&normalized) else {
        return Ok(ParsedFrontmatter {
            frontmatter: JsValue::Object(JsObject::new()),
            body: normalized,
        });
    };
    let frontmatter = if yaml_string.is_empty() {
        JsValue::Object(JsObject::new())
    } else {
        parse_yaml_frontmatter(&yaml_string)?
    };
    Ok(ParsedFrontmatter { frontmatter, body })
}

/// Return the body with frontmatter removed.
pub fn strip_frontmatter(content: impl Into<JsString>) -> Result<JsString, FrontmatterError> {
    Ok(parse_frontmatter(content)?.body)
}

fn extract_frontmatter(normalized: &JsString) -> Option<(JsString, JsString)> {
    if !normalized.starts_with("---") {
        return None;
    }
    let end_index = normalized.find_from("\n---", 3)?;
    let yaml_string = normalized.slice(4, end_index);
    let body = normalized.slice(end_index + 4, normalized.len()).trim();
    Some((yaml_string, body))
}

fn parse_yaml_frontmatter(yaml_string: &JsString) -> Result<JsValue, FrontmatterError> {
    // Temporary explicit restriction pending the native-YAML/gap decision. Body extraction
    // never crosses this scalar-only parser; neither literal nor escaped lone units are dropped.
    let mut document = yaml_string.to_utf8().map_err(|_| FrontmatterError {
        message: "YAML frontmatter contains an unpaired UTF-16 unit".into(),
    })?;
    if !document.ends_with('\n') {
        document.push('\n');
    }
    let yaml = serde_yaml_ng::from_str::<YamlValue>(&document).map_err(|error| FrontmatterError {
        message: error.to_string().into(),
    })?;
    if yaml.is_null() {
        return Ok(JsValue::Object(JsObject::new()));
    }
    yaml_to_js_value(yaml)
}

fn yaml_to_js_value(value: YamlValue) -> Result<JsValue, FrontmatterError> {
    match value {
        YamlValue::Null => Ok(JsValue::Null),
        YamlValue::Bool(value) => Ok(JsValue::Bool(value)),
        YamlValue::Number(value) => Ok(yaml_number_to_js_value(value)),
        YamlValue::String(value) => Ok(JsValue::String(value.into())),
        YamlValue::Sequence(values) => values
            .into_iter()
            .map(yaml_to_js_value)
            .collect::<Result<Vec<_>, _>>()
            .map(JsValue::Array),
        YamlValue::Mapping(mapping) => {
            let mut object = JsObject::new();
            for (key, value) in mapping {
                object.insert(yaml_key(key)?, yaml_to_js_value(value)?);
            }
            Ok(JsValue::Object(object))
        }
        YamlValue::Tagged(tagged) => Err(FrontmatterError {
            message: format!("unsupported YAML tag: {}", tagged.tag).into(),
        }),
    }
}

fn yaml_number_to_js_value(value: serde_yaml_ng::Number) -> JsValue {
    if let Some(value) = value.as_i64() {
        return JsValue::Number(value as f64);
    }
    if let Some(value) = value.as_u64() {
        return JsValue::Number(value as f64);
    }
    value
        .as_f64()
        .map_or(JsValue::Null, |value| js_number_value(value).into())
}

fn yaml_key(value: YamlValue) -> Result<JsString, FrontmatterError> {
    match value {
        YamlValue::String(value) => Ok(value.into()),
        YamlValue::Bool(value) => Ok(value.to_string().into()),
        YamlValue::Number(value) => Ok(value.to_string().into()),
        YamlValue::Null => Ok("null".into()),
        other => Err(FrontmatterError {
            message: format!("unsupported YAML mapping key: {other:?}").into(),
        }),
    }
}
