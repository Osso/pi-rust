use pi_js_compat::number::js_number_value;
use serde_json::{Map, Number, Value};
use serde_yaml_ng::Value as YamlValue;

/// Parsed frontmatter value and the trimmed body that follows it.
pub(crate) struct ParsedFrontmatter {
    pub(crate) frontmatter: Value,
    pub(crate) body: String,
}

/// Frontmatter YAML could not be parsed.
pub(crate) struct FrontmatterError {
    message: String,
}

impl FrontmatterError {
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

/// Parse YAML frontmatter delimited by a leading `---` and the first `\n---`.
///
/// Missing or unterminated frontmatter yields an empty object and the newline-normalized content.
pub(crate) fn parse_frontmatter(content: &str) -> Result<ParsedFrontmatter, FrontmatterError> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let Some((yaml_string, body)) = extract_frontmatter(&normalized) else {
        return Ok(ParsedFrontmatter {
            frontmatter: Value::Object(Map::new()),
            body: normalized,
        });
    };
    if yaml_string.is_empty() {
        return Ok(ParsedFrontmatter {
            frontmatter: Value::Object(Map::new()),
            body,
        });
    }

    let yaml = serde_yaml_ng::from_str::<YamlValue>(&document_text(yaml_string)).map_err(|error| FrontmatterError {
        message: error.to_string(),
    })?;
    if yaml.is_null() {
        return Ok(ParsedFrontmatter {
            frontmatter: Value::Object(Map::new()),
            body,
        });
    }
    Ok(ParsedFrontmatter {
        frontmatter: yaml_to_json(yaml)?,
        body,
    })
}

fn extract_frontmatter(normalized: &str) -> Option<(&str, String)> {
    if !normalized.starts_with("---") {
        return None;
    }
    let end_index = normalized.get(3..)?.find("\n---")? + 3;
    let yaml_string = if end_index >= 4 {
        normalized.get(4..end_index)?
    } else {
        ""
    };
    let body = normalized.get(end_index + 4..)?.trim().to_owned();
    Some((yaml_string, body))
}

fn document_text(yaml_string: &str) -> String {
    // Extraction removes the line break before the closing `---`; block scalars use it for chomping.
    if yaml_string.ends_with('\n') {
        yaml_string.to_owned()
    } else {
        format!("{yaml_string}\n")
    }
}

fn yaml_to_json(value: YamlValue) -> Result<Value, FrontmatterError> {
    match value {
        YamlValue::Null => Ok(Value::Null),
        YamlValue::Bool(value) => Ok(Value::Bool(value)),
        YamlValue::Number(value) => Ok(yaml_number_to_json(value)),
        YamlValue::String(value) => Ok(Value::String(value)),
        YamlValue::Sequence(values) => values
            .into_iter()
            .map(yaml_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        YamlValue::Mapping(mapping) => {
            let mut object = Map::new();
            for (key, value) in mapping {
                object.insert(yaml_key(key)?, yaml_to_json(value)?);
            }
            Ok(Value::Object(object))
        }
        YamlValue::Tagged(tagged) => Err(FrontmatterError {
            message: format!("unsupported YAML tag: {}", tagged.tag),
        }),
    }
}

fn yaml_number_to_json(value: serde_yaml_ng::Number) -> Value {
    if let Some(value) = value.as_i64() {
        return Value::Number(Number::from(value));
    }
    if let Some(value) = value.as_u64() {
        return Value::Number(Number::from(value));
    }
    value.as_f64().map_or(Value::Null, js_number_value)
}

fn yaml_key(value: YamlValue) -> Result<String, FrontmatterError> {
    match value {
        YamlValue::String(value) => Ok(value),
        YamlValue::Bool(value) => Ok(value.to_string()),
        YamlValue::Number(value) => Ok(value.to_string()),
        YamlValue::Null => Ok("null".to_owned()),
        other => Err(FrontmatterError {
            message: format!("unsupported YAML mapping key: {other:?}"),
        }),
    }
}
