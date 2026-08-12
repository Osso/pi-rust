use pi_agent_core::harness::utils::{TruncatedBy, TruncationOptions, TruncationResult, truncate_head, truncate_tail};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub(crate) enum Direction {
    Head,
    Tail,
}

impl Direction {
    fn operation(self) -> &'static str {
        match self {
            Self::Head => "truncate_head",
            Self::Tail => "truncate_tail",
        }
    }

    pub(crate) fn truncate(self, content: &str, options: Option<TruncationOptions>) -> TruncationResult {
        let options = options.unwrap_or_default();
        match self {
            Self::Head => truncate_head(content, options),
            Self::Tail => truncate_tail(content, options),
        }
    }
}

pub(crate) struct TruncationCase {
    pub(crate) name: &'static str,
    pub(crate) direction: Direction,
    pub(crate) content: String,
    pub(crate) options: Option<TruncationOptions>,
}

impl TruncationCase {
    pub(crate) fn new(
        name: &'static str,
        direction: Direction,
        content: impl Into<String>,
        options: Option<TruncationOptions>,
    ) -> Self {
        Self {
            name,
            direction,
            content: content.into(),
            options,
        }
    }

    pub(crate) fn request(&self, id: u64) -> Value {
        let mut request = json!({
            "id": id,
            "operation": self.direction.operation(),
            "content": self.content,
        });
        if let Some(options) = self.options {
            request["options"] = normalize_options(options);
        }
        request
    }

    pub(crate) fn rust_response(&self, id: u64) -> Value {
        let result = self.direction.truncate(&self.content, self.options);
        ok_response(id, normalize_result(&result))
    }
}

pub(crate) const fn options(max_lines: usize, max_bytes: usize) -> TruncationOptions {
    TruncationOptions { max_lines, max_bytes }
}

pub(crate) fn normalize_options(options: TruncationOptions) -> Value {
    json!({"maxLines": options.max_lines, "maxBytes": options.max_bytes})
}

pub(crate) fn normalize_result(result: &TruncationResult) -> Value {
    json!({
        "content": result.content,
        "truncated": result.truncated,
        "truncatedBy": match result.truncated_by {
            Some(TruncatedBy::Lines) => json!("lines"),
            Some(TruncatedBy::Bytes) => json!("bytes"),
            None => Value::Null,
        },
        "totalLines": result.total_lines,
        "totalBytes": result.total_bytes,
        "outputLines": result.output_lines,
        "outputBytes": result.output_bytes,
        "lastLinePartial": result.last_line_partial,
        "firstLineExceedsLimit": result.first_line_exceeds_limit,
        "maxLines": result.max_lines,
        "maxBytes": result.max_bytes,
    })
}

pub(crate) fn ok_response(id: u64, output: Value) -> Value {
    json!({"id": id, "ok": true, "output": output})
}
