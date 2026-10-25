//! Opt-in subagent discovery. `agent_dir` is the host's resolved private/configured agent directory.
use pi_coding_agent::utils::frontmatter::parse_frontmatter;
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub name: JsString,
    pub description: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<JsString>,
    pub system_prompt: JsString,
    pub source: JsString,
    pub file_path: PathBuf,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDiscoveryResult {
    pub agents: Vec<AgentConfig>,
    pub project_agents_dir: Option<PathBuf>,
}

pub fn discover_agents(cwd: &Path, scope: &JsString, agent_dir: &Path) -> Result<AgentDiscoveryResult, JsString> {
    let project_agents_dir = cwd
        .ancestors()
        .map(|dir| {
            dir.join(pi_js_compat::js_json::encode_node_utf8(
                pi_coding_agent::config::config_dir_name(),
            ))
            .join("agents")
        })
        .find(|dir| dir.is_dir());
    let mut agents = Vec::<AgentConfig>::new();
    if scope != "project" {
        merge_agents(&mut agents, load_agents(&agent_dir.join("agents"), "user")?);
    }
    if scope != "user"
        && let Some(dir) = &project_agents_dir
    {
        merge_agents(&mut agents, load_agents(dir, "project")?);
    }
    Ok(AgentDiscoveryResult {
        agents,
        project_agents_dir,
    })
}
fn merge_agents(target: &mut Vec<AgentConfig>, incoming: Vec<AgentConfig>) {
    for agent in incoming {
        if let Some(index) = target.iter().position(|old| old.name == agent.name) {
            target[index] = agent;
        } else {
            target.push(agent);
        }
    }
}
fn load_agents(dir: &Path, source: &str) -> Result<Vec<AgentConfig>, JsString> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    let mut agents = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !entry.file_name().to_string_lossy().ends_with(".md") {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() && !kind.is_symlink() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let parsed =
            parse_frontmatter(JsString::from(String::from_utf8_lossy(&bytes).as_ref())).map_err(|e| e.to_string())?;
        let f = parsed.frontmatter;
        let (Some(name), Some(description)) = (
            f["name"].as_string().filter(|s| !s.is_empty()),
            f["description"].as_string().filter(|s| !s.is_empty()),
        ) else {
            continue;
        };
        let tools = f["tools"]
            .as_string()
            .map(|s| {
                s.split(',')
                    .map(|text| text.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty());
        agents.push(AgentConfig {
            name: name.into(),
            description: description.into(),
            tools,
            model: f["model"].as_string().cloned(),
            system_prompt: parsed.body,
            source: source.into(),
            file_path: path,
        });
    }
    Ok(agents)
}
pub fn format_agent_list(agents: &[AgentConfig], max_items: i64) -> Value {
    if agents.is_empty() {
        return json!({"text":"none","remaining":0});
    }
    let end = if max_items < 0 {
        (agents.len() as i64 + max_items).max(0) as usize
    } else {
        (max_items as usize).min(agents.len())
    };
    json!({"text":JsString::join(agents[..end].iter().map(|a| js_format!("{} ({}): {}", a.name, a.source, a.description)), "; "), "remaining":agents.len()-end})
}
