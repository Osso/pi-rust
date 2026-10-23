//! Opt-in full-context compaction using the selected registry's Gemini Flash model.
use pi_agent_core::uuidv7;
use pi_ai::{
    request::StreamOptions,
    types::{
        AssistantContent, CacheRetention, Context, Message, TextContent, UserContent, UserContentBlock, UserMessage,
        UserRole,
    },
};
use pi_coding_agent::core::{compaction::serialize_conversation, extensions::*, messages::convert_to_llm};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

pub fn custom_compaction_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_compaction(|event:CompactionEvent,ctx:ExtensionContext|->BoxFuture<'static,Result<Option<CompactionEventResult>,HandlerError>> {
        Box::pin(async move {
            let ui=ctx.ui()?;
            ui.notify(&JsString::from("Custom compaction extension triggered"),Some(NotifyType::Info));
            let registry=ctx.model_registry()?;
            let Some(model)=registry.find("google","gemini-2.5-flash")else{
                ui.notify(&JsString::from("Could not find Gemini Flash model, using default compaction"),Some(NotifyType::Warning));return Ok(None);
            };
            let preparation=event.preparation;
            let mut messages=preparation.messages_to_summarize;messages.extend(preparation.turn_prefix_messages);
            ui.notify(&JsString::from(format!("Custom compaction: summarizing {} messages ({} tokens) with {}...",messages.len(),format_tokens(preparation.tokens_before),model.id)),Some(NotifyType::Info));
            let conversation_text=serialize_conversation(&convert_to_llm(&messages).map_err(|error| HandlerError::new(error.message()))?);
            let previous_context=preparation.previous_summary.filter(|s|!s.is_empty()).map(|s|js_format!("\n\nPrevious session summary for context:\n{}", s)).unwrap_or_default();
            let prompt=js_format!(r#"You are a conversation summarizer. Create a comprehensive summary of this conversation that captures:{}

1. The main goals and objectives discussed
2. Key decisions made and their rationale
3. Important code changes, file modifications, or technical details
4. Current state of any ongoing work
5. Any blockers, issues, or open questions
6. Next steps that were planned or suggested

Be thorough but concise. The summary will replace the ENTIRE conversation history, so include all information needed to continue the work effectively.

Format the summary as structured markdown with clear sections.

<conversation>
{}
</conversation>"#, previous_context, conversation_text);
            let user=Message::User(UserMessage{role:UserRole::Tag,content:UserContent::Blocks(vec![UserContentBlock::Text(TextContent::new(prompt))]),input_source:None,provider_native:None,timestamp:pi_js_compat::date::js_now_millis()});
            let signal=event.signal;
            let mut options=StreamOptions {max_tokens:Some(8192.0),cache_retention:Some(CacheRetention::None),session_id:Some(JsString::from(uuidv7())),..Default::default()};
            options.request.signal=signal.clone();
            let response=match registry.complete(model,Context{system_prompt:None,messages:vec![user],tools:None},options).await {
                Ok(response)=>response,
                Err(error)=>{ui.notify(&JsString::from(format!("Compaction failed: {error}")),Some(NotifyType::Error));return Ok(None);}
            };
            let summary=JsString::join(response.content.iter().filter_map(|c|match c{AssistantContent::Text(t)=>Some(t.text.clone()),_=>None}), "\n");
            if summary.trim().is_empty(){
                if !signal.is_some_and(|s|s.aborted()){ui.notify(&JsString::from("Compaction summary was empty, using default compaction"),Some(NotifyType::Warning));}
                return Ok(None);
            }
            let compaction=pi_js_compat::js_json::from_value(&pi_js_compat::js_json!({"summary":summary,"firstKeptEntryId":preparation.first_kept_entry_id,"tokensBefore":preparation.tokens_before,"usage":response.usage})).map_err(|e|HandlerError::new(e.to_string()))?;
            Ok(Some(CompactionEventResult {compaction:Some(compaction),..Default::default()}))
        })
    }).map_err(HandlerError::new)
}
// Intl's default en-US decimal formatter rounds the shortest decimal to three
// fractional places (not the exact binary fraction used by Number.toFixed).
fn format_tokens(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if value.is_infinite() {
        return format!("{sign}∞");
    }
    let decimal = pi_js_compat::number::js_number_to_string(value.abs());
    let mut scaled = round_decimal_thousandths(&decimal);
    if scaled.len() < 4 {
        scaled = format!("{}{}", "0".repeat(4 - scaled.len()), scaled);
    }
    let (integer, fraction) = scaled.split_at(scaled.len() - 3);
    let fraction = fraction.trim_end_matches('0');
    let grouped = integer
        .chars()
        .enumerate()
        .flat_map(|(index, digit)| {
            let comma = index > 0 && (integer.len() - index) % 3 == 0;
            comma.then_some(',').into_iter().chain(std::iter::once(digit))
        })
        .collect::<String>();
    let suffix = if fraction.is_empty() {
        String::new()
    } else {
        format!(".{fraction}")
    };
    format!("{sign}{grouped}{suffix}")
}
fn round_decimal_thousandths(decimal: &str) -> String {
    let (mantissa, exponent) = decimal
        .split_once('e')
        .map_or((decimal, 0), |(m, e)| (m, e.parse::<i32>().expect("numeric exponent")));
    let integer_len = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let mut digits = mantissa.bytes().filter(|c| *c != b'.').collect::<Vec<_>>();
    let kept = integer_len + exponent + 3;
    if kept < 0 {
        return "0".into();
    }
    let kept = kept as usize;
    let round_up = digits.get(kept).is_some_and(|digit| *digit >= b'5');
    digits.resize(kept, b'0');
    if round_up {
        increment_decimal(&mut digits);
    }
    if digits.is_empty() {
        return "0".into();
    }
    String::from_utf8(digits).expect("ASCII decimal")
}
fn increment_decimal(digits: &mut Vec<u8>) {
    for digit in digits.iter_mut().rev() {
        if *digit < b'9' {
            *digit += 1;
            return;
        }
        *digit = b'0';
    }
    digits.insert(0, b'1');
}
