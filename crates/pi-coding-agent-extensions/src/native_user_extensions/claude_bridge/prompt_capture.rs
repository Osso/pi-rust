//! Source prompt-capture.ts and agents-md.ts; captures never forward Pi's harness.
use crate::core::skills::{Skill, format_skills_for_prompt};
use crate::core::system_prompt::{BuildSystemPromptOptions, ContextFile};
use indexmap::IndexMap;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

type CaptureRef = Arc<Mutex<Capture>>;

#[derive(Clone, Default)]
struct CaptureInput {
    custom: Option<JsString>,
    append: Option<JsString>,
    context_files: Vec<ContextFile>,
    skills: Vec<Skill>,
    sections: JsObject,
}

#[derive(Clone)]
struct Edge {
    start: usize,
    end: usize,
    parent: CaptureRef,
}

#[derive(Clone)]
struct Capture {
    prompt: JsString,
    input: CaptureInput,
    source: Option<String>,
    inherited: Vec<Edge>,
}

#[derive(Default)]
pub struct PromptCaptures {
    captures: IndexMap<JsString, CaptureRef>,
}

pub fn shared_captures() -> Arc<Mutex<PromptCaptures>> {
    static CAPTURES: OnceLock<Arc<Mutex<PromptCaptures>>> = OnceLock::new();
    CAPTURES
        .get_or_init(|| Arc::new(Mutex::new(PromptCaptures::default())))
        .clone()
}

// Approved mapping (2026-10-25): frozen pi's rulesContent occupies the source sections slot.
fn capture_input(options: &BuildSystemPromptOptions) -> CaptureInput {
    let mut sections = JsObject::new();
    if let Some(rules) = &options.rules_content {
        sections.insert("user_rules".into(), JsValue::String(rules.clone()));
    }
    let has_read = options
        .selected_tools
        .as_ref()
        .is_none_or(|tools| tools.iter().any(|v| v == "read"));
    CaptureInput {
        custom: options.custom_prompt.clone(),
        append: options.append_system_prompt.clone(),
        context_files: options.context_files.clone().unwrap_or_default(),
        skills: if has_read {
            options.skills.clone().unwrap_or_default()
        } else {
            Vec::new()
        },
        sections,
    }
}

fn snapshot(node: &CaptureRef) -> Capture {
    node.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
}

impl PromptCaptures {
    pub fn record(&mut self, prompt: &JsString, options: &BuildSystemPromptOptions, source: &str) {
        if prompt.is_empty() {
            return;
        }
        let input = capture_input(options);
        let existing = self.captures.get(prompt).cloned();
        let inherited = if existing
            .as_ref()
            .is_some_and(|node| snapshot(node).input.custom == input.custom)
        {
            existing
                .as_ref()
                .map(|node| snapshot(node).inherited)
                .unwrap_or_default()
        } else {
            self.find_inherited(prompt, input.custom.as_ref())
        };
        let capture = Capture {
            prompt: prompt.clone(),
            input,
            source: Some(source.to_owned()),
            inherited,
        };
        let node = if let Some(node) = existing {
            *node.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = capture;
            node
        } else {
            Arc::new(Mutex::new(capture))
        };
        self.touch(prompt, node);
    }

    pub fn project(&mut self, prompt: Option<&JsString>, read_tool: &str) -> Result<Option<JsString>, CallbackError> {
        let Some(node) = self.resolve(prompt)? else {
            return Ok(None);
        };
        project_capture(&node, read_tool, &mut HashSet::new())
    }

    pub fn skills(&mut self, prompt: Option<&JsString>, read_tool: &str) -> Result<Option<JsString>, CallbackError> {
        let Some(node) = self.resolve(prompt)? else {
            return Ok(None);
        };
        let skills = collect_skills(&node, &mut HashSet::new(), &mut HashSet::new())?;
        Ok(render_skills(&skills, read_tool))
    }

    fn touch(&mut self, prompt: &JsString, node: CaptureRef) {
        self.captures.shift_remove(prompt);
        self.captures.insert(prompt.clone(), node);
        while self.captures.len() > 256 {
            self.captures.shift_remove_index(0);
        }
    }

    fn resolve(&mut self, prompt: Option<&JsString>) -> Result<Option<CaptureRef>, CallbackError> {
        let Some(prompt) = prompt.filter(|p| !p.is_empty()) else {
            return Ok(None);
        };
        let exact = self.captures.get(prompt).cloned().or_else(|| {
            self.reachable()
                .into_iter()
                .find(|node| snapshot(node).prompt == *prompt)
        });
        if let Some(node) = exact {
            self.touch(prompt, node.clone());
            return Ok(Some(node));
        }
        let inherited = self.find_inherited(prompt, Some(prompt));
        if inherited.is_empty() {
            return Err(self.unresolved_error(prompt));
        }
        Ok(Some(Arc::new(Mutex::new(Capture {
            prompt: prompt.clone(),
            input: CaptureInput {
                custom: Some(prompt.clone()),
                ..Default::default()
            },
            source: None,
            inherited,
        }))))
    }

    fn reachable(&self) -> Vec<CaptureRef> {
        let mut nodes = Vec::new();
        let mut visited = HashSet::new();
        let mut stack: Vec<_> = self.captures.values().rev().cloned().collect();
        while let Some(node) = stack.pop() {
            if !visited.insert(Arc::as_ptr(&node) as usize) {
                continue;
            }
            stack.extend(snapshot(&node).inherited.iter().rev().map(|edge| edge.parent.clone()));
            nodes.push(node);
        }
        nodes
    }

    fn find_inherited(&self, prompt: &JsString, custom: Option<&JsString>) -> Vec<Edge> {
        let Some(custom) = custom.filter(|c| !c.is_empty()) else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for node in self.reachable() {
            let key = snapshot(&node).prompt;
            if key == *prompt || key.is_empty() {
                continue;
            }
            let mut cursor = 0;
            while let Some(start) = custom.find_from(&key, cursor) {
                let end = start + key.len();
                candidates.push(Edge {
                    start,
                    end,
                    parent: node.clone(),
                });
                cursor = end;
            }
        }
        candidates.sort_by(|a, b| (b.end - b.start).cmp(&(a.end - a.start)).then(a.start.cmp(&b.start)));
        let mut selected: Vec<Edge> = Vec::new();
        for candidate in candidates {
            if !selected
                .iter()
                .any(|e| candidate.start < e.end && candidate.end > e.start)
            {
                selected.push(candidate);
            }
        }
        selected.sort_by_key(|edge| edge.start);
        selected
    }

    fn unresolved_error(&self, prompt: &JsString) -> CallbackError {
        let matches: Vec<_> = self
            .captures
            .iter()
            .map(|(key, node)| {
                let prefix = key
                    .units()
                    .iter()
                    .zip(prompt.units())
                    .take_while(|(a, b)| a == b)
                    .count();
                (key, snapshot(node), prefix)
            })
            .collect();
        let shared = matches.iter().map(|(_, _, prefix)| *prefix).max().unwrap_or(0);
        let best: Vec<_> = matches.iter().filter(|(_, _, prefix)| *prefix == shared).collect();
        if let Some((key, node, _)) = best
            .iter()
            .find(|(key, _, prefix)| *prefix == prompt.len() && key.len() > prompt.len())
        {
            return CallbackError::new(format!(
                "prompt-capture: this {}-char prompt is the {}-char capture recorded at {} with its final {} chars missing. That shape matches pi#5581: an extension-triggered idle turn (sendMessage with triggerTurn) skips before_agent_start, so the prompt lacks that turn's extension additions. The turn fails; the next user-typed turn resolves normally. Workaround: when idle, send a user message instead of triggerTurn.",
                prompt.len(),
                key.len(),
                node.source.as_deref().unwrap_or("an unknown boundary"),
                key.len() - prompt.len()
            ));
        }
        let offset = best.first().map_or("?".to_owned(), |(_, _, prefix)| prefix.to_string());
        let length = best.first().map_or(0, |(key, _, _)| key.len());
        let source = best
            .first()
            .and_then(|(_, node, _)| node.source.as_ref())
            .map(|s| format!(", last recorded at {s}"))
            .unwrap_or_default();
        CallbackError::new(format!(
            "prompt-capture: no capture for this {}-char system prompt, and it embeds none of the {} known. Closest known match diverges at offset {} ({}-char key{}). Claude Code would receive none of this turn's context files, skills or custom instructions. When the divergence sits at a section's opening tag, the capture and this prompt disagree about that section — pi's mcp_servers since pi 0.99.2, or one an extension added. Otherwise an extension loaded after claude-bridge rewrote the system prompt from before_agent_start — one that wraps it is fine, one that rebuilds or strips it leaves nothing to match — or pi rebuilt the prompt outside before_agent_start (a late-registered tool or fresh resource discovery).",
            prompt.len(),
            self.captures.len(),
            offset,
            length,
            source
        ))
    }
}

fn collect_skills(
    node: &CaptureRef,
    seen: &mut HashSet<JsString>,
    visiting: &mut HashSet<usize>,
) -> Result<Vec<Skill>, CallbackError> {
    let key = Arc::as_ptr(node) as usize;
    if !visiting.insert(key) {
        return Err(CallbackError::new("Cyclic prompt inheritance"));
    }
    let capture = snapshot(node);
    let mut skills = Vec::new();
    for edge in &capture.inherited {
        skills.extend(collect_skills(&edge.parent, seen, visiting)?);
    }
    for skill in capture.input.skills {
        if !skill.disable_model_invocation && seen.insert(skill.file_path.clone()) {
            skills.push(skill);
        }
    }
    visiting.remove(&key);
    Ok(skills)
}

fn render_skills(skills: &[Skill], read_tool: &str) -> Option<JsString> {
    if read_tool == "none" {
        return None;
    }
    let text = format_skills_for_prompt(skills).trim();
    if text.is_empty() {
        return None;
    }
    Some(if read_tool == "mcp" {
        text.replace(
            "Use the read tool to load a skill's file",
            "Use the read tool (mcp__custom-tools__read) to load a skill's file",
        )
    } else {
        text
    })
}

fn project_capture(
    node: &CaptureRef,
    read_tool: &str,
    visiting: &mut HashSet<usize>,
) -> Result<Option<JsString>, CallbackError> {
    let key = Arc::as_ptr(node) as usize;
    if !visiting.insert(key) {
        return Err(CallbackError::new("Cyclic prompt inheritance"));
    }
    let capture = snapshot(node);
    let mut seen = HashSet::new();
    for edge in &capture.inherited {
        collect_skills(&edge.parent, &mut seen, &mut HashSet::new())?;
    }
    let own_skills: Vec<_> = capture
        .input
        .skills
        .iter()
        .filter(|s| !s.disable_model_invocation && seen.insert(s.file_path.clone()))
        .cloned()
        .collect();
    let mut parts = Vec::new();
    if let Some(context) = project_context(&capture.input.context_files) {
        parts.push(("the project context block".to_owned(), context));
    }
    if let Some(skills) = render_skills(&own_skills, read_tool) {
        parts.push(("the skills block".to_owned(), skills));
    }
    if let Some(custom) = project_custom(&capture, read_tool, visiting)? {
        parts.push(("the custom prompt".to_owned(), custom));
    }
    if let Some(append) = &capture.input.append
        && !append.is_empty()
    {
        parts.push(("the appended instructions".to_owned(), append.clone()));
    }
    for (name, content) in &capture.input.sections {
        if let Some(content) = content.as_string().filter(|v| !v.is_empty()) {
            parts.push((
                pi_js_compat::js_json::encode_node_utf8(&pi_js_compat::js_format!("the {} section", name)),
                pi_js_compat::js_format!("<{}>\n{}\n</{}>", name, content, name),
            ));
        }
    }
    assert_sendable(&parts, &capture)?;
    visiting.remove(&key);
    Ok((!parts.is_empty()).then(|| JsString::join(parts.into_iter().map(|(_, text)| text), "\n\n")))
}

fn project_custom(
    capture: &Capture,
    read_tool: &str,
    visiting: &mut HashSet<usize>,
) -> Result<Option<JsString>, CallbackError> {
    let Some(custom) = capture.input.custom.as_ref().filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let mut result = JsString::default();
    let mut cursor = 0;
    for edge in &capture.inherited {
        result.push_str(custom.slice(cursor, edge.start));
        if let Some(parent) = project_capture(&edge.parent, read_tool, visiting)? {
            result.push_str(parent);
        }
        cursor = edge.end;
    }
    result.push_str(custom.slice(cursor, custom.len()));
    Ok((!result.is_empty()).then_some(result))
}

fn project_context(files: &[ContextFile]) -> Option<JsString> {
    if files.is_empty() {
        return None;
    }
    let mut prompt = JsString::from("<project_context>\n\nProject-specific instructions and guidelines:\n\n");
    for file in files {
        prompt.push_str(pi_js_compat::js_format!(
            "<project_instructions path=\"{}\">\n{}\n</project_instructions>\n\n",
            file.path,
            file.content
        ));
    }
    prompt.push_str("</project_context>");
    Some(prompt)
}

fn assert_sendable(parts: &[(String, JsString)], capture: &Capture) -> Result<(), CallbackError> {
    const PREAMBLE: &str = "You are an expert coding assistant operating inside pi";
    let mut findings = Vec::new();
    for (label, text) in parts {
        let mut cursor = 0;
        while let Some(offset) = text.find_from(PREAMBLE, cursor) {
            if offset == 0 || text.units()[offset - 1] == u16::from(b'\n') {
                findings.push(format!(
                    "pi's preamble (\"{PREAMBLE}\") in {label}, at offset {offset} of {} chars",
                    text.len()
                ));
                break;
            }
            cursor = offset + PREAMBLE.len();
        }
        if text.contains("docs/custom-provider.md") && text.contains("docs/packages.md") {
            findings.push(format!("docs/custom-provider.md and docs/packages.md in {label}"));
        }
    }
    if findings.is_empty() {
        return Ok(());
    }
    let log_path = std::env::var("CLAUDE_BRIDGE_DEBUG_PATH")
        .ok()
        .filter(|s| !s.is_empty())
        .map(JsString::from)
        .map(Ok)
        .unwrap_or_else(|| {
            crate::config::get_agent_dir()
                .map(|dir| pi_js_compat::js_format!("{}/claude-bridge.log", dir))
                .map_err(|error| CallbackError::new(error.to_string()))
        })?;
    let mut lines = vec![
        "prompt-capture: refusing to send this prompt. Claude Code's Anthropic path reads a request".into(),
        "  carrying pi's harness, or the phrase pair its subscription gate rejects, as a third-party".into(),
        "  app: it fails with 400 or is billed as extra usage.".into(),
    ];
    lines.extend(
        findings
            .into_iter()
            .map(|finding| JsString::from(format!("  Found: {finding}."))),
    );
    lines.push(
        format!(
            "  Capture: {}, {} inherited capture(s) substituted.",
            capture.source.as_deref().unwrap_or("unknown"),
            capture.inherited.len()
        )
        .into(),
    );
    lines.extend([
        "  If this came from an inherited pi prompt, see README \"Compatibility with other extensions\".".into(),
        "  If it is your own text, reword or remove it. CLAUDE_BRIDGE_DEBUG=1 writes the full prompt to".into(),
        pi_js_compat::js_format!("  {}.", log_path),
    ]);
    Err(CallbackError::new(JsString::join(lines, "\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::system_prompt::ContextFile;

    #[test]
    fn claude_bridge_prompt_exact_rules_present_and_absent() {
        let mut captures = PromptCaptures::default();
        let mut options = BuildSystemPromptOptions {
            custom_prompt: Some("Portable custom".into()),
            append_system_prompt: Some("Appended policy".into()),
            rules_content: Some("Never disclose secrets.\nKeep UTF-16 😀".into()),
            ..Default::default()
        };
        captures.record(&"assembled with rules".into(), &options, "before_agent_start");
        assert_eq!(captures.project(Some(&"assembled with rules".into()),"none").unwrap(),Some("Portable custom\n\nAppended policy\n\n<user_rules>\nNever disclose secrets.\nKeep UTF-16 😀\n</user_rules>".into()));
        options.rules_content = None;
        captures.record(&"assembled without rules".into(), &options, "before_agent_start");
        assert_eq!(
            captures
                .project(Some(&"assembled without rules".into()), "none")
                .unwrap(),
            Some("Portable custom\n\nAppended policy".into())
        );
    }

    #[test]
    fn claude_bridge_prompt_inheritance_replaces_harness_and_preserves_wrappers() {
        let mut captures = PromptCaptures::default();
        let parent =
            JsString::from("You are an expert coding assistant operating inside pi\nA complete parent's prompt");
        captures.record(
            &parent,
            &BuildSystemPromptOptions {
                context_files: Some(vec![ContextFile {
                    path: "/repo/AGENTS.md".into(),
                    content: "Be exact.".into(),
                }]),
                ..Default::default()
            },
            "before_agent_start",
        );
        let child = pi_js_compat::js_format!("Child task\n{}\nChild suffix", parent);
        let projected = captures.project(Some(&child), "none").unwrap().unwrap();
        assert_eq!(
            projected,
            "Child task\n<project_context>\n\nProject-specific instructions and guidelines:\n\n<project_instructions path=\"/repo/AGENTS.md\">\nBe exact.\n</project_instructions>\n\n</project_context>\nChild suffix"
        );
        assert!(!projected.contains("operating inside pi"));
    }

    #[test]
    fn claude_bridge_prompt_unknown_capture_fails_explicitly() {
        let mut captures = PromptCaptures::default();
        let error = captures.project(Some(&"unrecorded".into()), "mcp").unwrap_err();
        assert!(
            error
                .message
                .starts_with("prompt-capture: no capture for this 10-char system prompt")
        );
    }
}
