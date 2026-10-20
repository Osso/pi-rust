use super::{failed, output, text};
use pi_agent_core::AgentToolResult;
use pi_coding_agent::{
    core::extensions::{ToolRenderContext, ToolRenderResultOptions},
    modes::interactive::theme::{Theme, ThemeColor as C, get_markdown_theme},
};
use pi_js_compat::string::js_trim;
use pi_tui::{
    Markdown, Spacer, Text,
    tui::{ComponentRef, Container, component},
};
use serde_json::Value;
fn txt(s: impl Into<String>) -> ComponentRef {
    component(Text::new(s, 0, 0))
}
fn preview(s: &str, n: usize) -> String {
    let units = s.encode_utf16().collect::<Vec<_>>();
    if units.len() > n {
        format!("{}...", String::from_utf16_lossy(&units[..n]))
    } else {
        s.into()
    }
}
fn number(n: f64) -> String {
    pi_js_compat::number::js_number_to_string(n)
}
fn tokens(n: f64) -> String {
    if n < 1000.0 {
        number(n)
    } else if n < 10000.0 {
        format!("{:.1}k", n / 1000.0)
    } else if n < 1000000.0 {
        format!("{}k", number((n / 1000.0 + 0.5).floor()))
    } else {
        format!("{:.1}M", n / 1000000.0)
    }
}
fn usage(v: &Value, model: Option<&str>) -> String {
    let mut parts = vec![];
    let turns = v["turns"].as_f64().unwrap_or(0.0);
    if turns != 0.0 {
        parts.push(format!("{} turn{}", number(turns), if turns > 1.0 { "s" } else { "" }));
    }
    for (key, prefix) in [("input", "↑"), ("output", "↓"), ("cacheRead", "R"), ("cacheWrite", "W")] {
        let n = v[key].as_f64().unwrap_or(0.0);
        if n != 0.0 {
            parts.push(format!("{prefix}{}", tokens(n)));
        }
    }
    let cost = v["cost"].as_f64().unwrap_or(0.0);
    if cost != 0.0 {
        parts.push(format!("${cost:.4}"));
    }
    let context = v["contextTokens"].as_f64().unwrap_or(0.0);
    if context > 0.0 {
        parts.push(format!("ctx:{}", tokens(context)));
    }
    if let Some(m) = model.filter(|m| !m.is_empty()) {
        parts.push(m.into());
    }
    parts.join(" ")
}
pub fn call(args: &Value, th: &Theme, _: &ToolRenderContext) -> ComponentRef {
    let scope = args["agentScope"].as_str().unwrap_or("user");
    for (key, label) in [("chain", "steps"), ("tasks", "tasks")] {
        if let Some(items) = args[key].as_array().filter(|a| !a.is_empty()) {
            let mode = if key == "chain" { "chain" } else { "parallel" };
            let mut s = th.fg(C::ToolTitle, &th.bold("subagent "))
                + &th.fg(C::Accent, &format!("{mode} ({} {label})", items.len()))
                + &th.fg(C::Muted, &format!(" [{scope}]"));
            for (i, item) in items.iter().take(3).enumerate() {
                let task = if key == "chain" {
                    js_trim(&text(&item["task"]).replace("{previous}", "")).to_owned()
                } else {
                    text(&item["task"]).into()
                };
                s += "\n  ";
                if key == "chain" {
                    s += &(th.fg(C::Muted, &format!("{}.", i + 1)) + " ");
                }
                s += &(th.fg(C::Accent, text(&item["agent"])) + &th.fg(C::Dim, &format!(" {}", preview(&task, 40))));
            }
            if items.len() > 3 {
                s += &format!("\n  {}", th.fg(C::Muted, &format!("... +{} more", items.len() - 3)));
            }
            return txt(s);
        }
    }
    let agent = args["agent"].as_str().filter(|s| !s.is_empty()).unwrap_or("...");
    let task = args["task"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| preview(s, 60))
        .unwrap_or_else(|| "...".into());
    txt(th.fg(C::ToolTitle, &th.bold("subagent "))
        + &th.fg(C::Accent, agent)
        + &th.fg(C::Muted, &format!(" [{scope}]"))
        + &format!("\n  {}", th.fg(C::Dim, &task)))
}
fn items(r: &Value) -> Vec<Value> {
    r["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["role"] == "assistant")
        .flat_map(|m| m["content"].as_array().into_iter().flatten())
        .filter(|p| p["type"] == "text" || p["type"] == "toolCall")
        .cloned()
        .collect()
}
fn tool(p: &Value, th: &Theme) -> String {
    let name = text(&p["name"]);
    let a = &p["arguments"];
    let mut path = a["file_path"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| a["path"].as_str().filter(|s| !s.is_empty()))
        .unwrap_or(if name == "find" || name == "grep" || name == "ls" {
            "."
        } else {
            "..."
        })
        .to_owned();
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        if path.starts_with(home.as_ref()) {
            path = format!("~{}", &path[home.len()..]);
        }
    }
    match name {
        "bash" => {
            th.fg(C::Muted, "$ ")
                + &th.fg(
                    C::ToolOutput,
                    &preview(a["command"].as_str().filter(|s| !s.is_empty()).unwrap_or("..."), 60),
                )
        }
        "read" => {
            let mut s = th.fg(C::Muted, "read ") + &th.fg(C::Accent, &path);
            if a.get("offset").is_some() || a.get("limit").is_some() {
                let start = a["offset"].as_i64().unwrap_or(1);
                let end = a["limit"]
                    .as_i64()
                    .map(|n| start + n - 1)
                    .filter(|n| *n != 0)
                    .map(|n| format!("-{n}"))
                    .unwrap_or_default();
                s += &th.fg(C::Warning, &format!(":{start}{end}"));
            }
            s
        }
        "write" => {
            let n = text(&a["content"]).split('\n').count();
            let mut s = th.fg(C::Muted, "write ") + &th.fg(C::Accent, &path);
            if n > 1 {
                s += &th.fg(C::Dim, &format!(" ({n} lines)"));
            }
            s
        }
        "edit" | "ls" => th.fg(C::Muted, &format!("{name} ")) + &th.fg(C::Accent, &path),
        "find" => {
            th.fg(C::Muted, "find ")
                + &th.fg(
                    C::Accent,
                    a["pattern"].as_str().filter(|s| !s.is_empty()).unwrap_or("*"),
                )
                + &th.fg(C::Dim, &format!(" in {path}"))
        }
        "grep" => {
            th.fg(C::Muted, "grep ")
                + &th.fg(C::Accent, &format!("/{}/", text(&a["pattern"])))
                + &th.fg(C::Dim, &format!(" in {path}"))
        }
        _ => th.fg(C::Accent, name) + &th.fg(C::Dim, &format!(" {}", preview(&a.to_string(), 50))),
    }
}
fn display(items: &[Value], limit: usize, expanded: bool, th: &Theme) -> String {
    let skip = items.len().saturating_sub(limit);
    let mut s = String::new();
    if skip > 0 {
        s += &th.fg(C::Muted, &format!("... {skip} earlier items\n"));
    }
    for item in &items[skip..] {
        if item["type"] == "text" {
            let t = text(&item["text"]);
            let t = if expanded {
                t.into()
            } else {
                t.split('\n').take(3).collect::<Vec<_>>().join("\n")
            };
            s += &th.fg(C::ToolOutput, &t);
        } else {
            s += &(th.fg(C::Muted, "→ ") + &tool(item, th));
        }
        s.push('\n');
    }
    s.trim_end().into()
}
fn spacer(c: &mut Container) {
    c.add_child(component(Spacer::new(1)));
}
fn expanded_output(c: &mut Container, r: &Value, th: &Theme, single: bool) {
    let display = items(r);
    let final_output = output(&r["messages"]);
    if single && display.is_empty() && final_output.is_empty() {
        c.add_child(txt(th.fg(C::Muted, "(no output)")));
        return;
    }
    for item in &display {
        if item["type"] == "toolCall" {
            c.add_child(txt(th.fg(C::Muted, "→ ") + &tool(item, th)));
        }
    }
    if !final_output.is_empty() {
        spacer(c);
        c.add_child(component(Markdown::new(
            js_trim(&final_output),
            0,
            0,
            get_markdown_theme(),
            None,
            Default::default(),
        )));
    }
}
fn aggregate(rs: &[Value]) -> Value {
    let mut v = serde_json::json!({"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"cost":0,"turns":0});
    for r in rs {
        for key in ["input", "output", "cacheRead", "cacheWrite", "cost", "turns"] {
            v[key] = serde_json::json!(v[key].as_f64().unwrap_or(0.0) + r["usage"][key].as_f64().unwrap_or(0.0));
        }
    }
    v
}
pub fn result(
    result: &AgentToolResult,
    options: &ToolRenderResultOptions,
    th: &Theme,
    _: &ToolRenderContext,
) -> ComponentRef {
    let fallback = || {
        txt(result
            .content
            .first()
            .and_then(|p| {
                if let pi_ai::types::TextOrImageContent::Text(t) = p {
                    Some(t.text.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "(no output)".into()))
    };
    let Some(d) = &result.details else {
        return fallback();
    };
    let Some(rs) = d["results"].as_array().filter(|r| !r.is_empty()) else {
        return fallback();
    };
    if d["mode"] == "single" && rs.len() == 1 {
        let r = &rs[0];
        let error = failed(r);
        let icon = th.fg(if error { C::Error } else { C::Success }, if error { "✗" } else { "✓" });
        let mut header = icon
            + " "
            + &th.fg(C::ToolTitle, &th.bold(text(&r["agent"])))
            + &th.fg(C::Muted, &format!(" ({})", text(&r["agentSource"])));
        if error && r["stopReason"].as_str().is_some_and(|s| !s.is_empty()) {
            header += &format!(" {}", th.fg(C::Error, &format!("[{}]", text(&r["stopReason"]))));
        }
        let stats = usage(&r["usage"], r["model"].as_str());
        if options.expanded {
            let mut c = Container::new();
            c.add_child(txt(header));
            if error && r["errorMessage"].as_str().is_some_and(|s| !s.is_empty()) {
                c.add_child(txt(th.fg(C::Error, &format!("Error: {}", text(&r["errorMessage"])))));
            }
            spacer(&mut c);
            c.add_child(txt(th.fg(C::Muted, "─── Task ───")));
            c.add_child(txt(th.fg(C::Dim, text(&r["task"]))));
            spacer(&mut c);
            c.add_child(txt(th.fg(C::Muted, "─── Output ───")));
            expanded_output(&mut c, r, th, true);
            if !stats.is_empty() {
                spacer(&mut c);
                c.add_child(txt(th.fg(C::Dim, &stats)));
            }
            return component(c);
        }
        let display_items = items(r);
        if error && r["errorMessage"].as_str().is_some_and(|s| !s.is_empty()) {
            header += &format!("\n{}", th.fg(C::Error, &format!("Error: {}", text(&r["errorMessage"]))));
        } else if display_items.is_empty() {
            header += &format!("\n{}", th.fg(C::Muted, "(no output)"));
        } else {
            header += &format!("\n{}", display(&display_items, 10, false, th));
            if display_items.len() > 10 {
                header += &format!("\n{}", th.fg(C::Muted, "(Ctrl+O to expand)"));
            }
        }
        if !stats.is_empty() {
            header += &format!("\n{}", th.fg(C::Dim, &stats));
        }
        return txt(header);
    }
    let chain = d["mode"] == "chain";
    if !chain && d["mode"] != "parallel" {
        return fallback();
    }
    let running = if chain {
        0
    } else {
        rs.iter().filter(|r| r["exitCode"] == -1).count()
    };
    let success = rs
        .iter()
        .filter(|r| {
            if chain {
                r["exitCode"] == 0
            } else {
                r["exitCode"] != -1 && !failed(r)
            }
        })
        .count();
    let fail = rs.len() - success - running;
    let (role, icon) = if chain {
        if success == rs.len() {
            (C::Success, "✓")
        } else {
            (C::Error, "✗")
        }
    } else if running > 0 {
        (C::Warning, "⏳")
    } else if fail > 0 {
        (C::Warning, "◐")
    } else {
        (C::Success, "✓")
    };
    let status = if chain {
        format!("{success}/{} steps", rs.len())
    } else if running > 0 {
        format!("{}/{} done, {running} running", success + fail, rs.len())
    } else {
        format!("{success}/{} tasks", rs.len())
    };
    let header = th.fg(role, icon)
        + " "
        + &th.fg(C::ToolTitle, &th.bold(if chain { "chain " } else { "parallel " }))
        + &th.fg(C::Accent, &status);
    let stats = usage(&aggregate(rs), None);
    if options.expanded && running == 0 {
        let mut c = Container::new();
        c.add_child(txt(header));
        for r in rs {
            spacer(&mut c);
            let ok = if chain { r["exitCode"] == 0 } else { !failed(r) };
            let icon = th.fg(if ok { C::Success } else { C::Error }, if ok { "✓" } else { "✗" });
            let prefix = if chain {
                format!("─── Step {}: ", r["step"])
            } else {
                "─── ".into()
            };
            let heading = th.fg(C::Muted, &prefix) + &th.fg(C::Accent, text(&r["agent"])) + " " + &icon;
            c.add_child(txt(heading));
            c.add_child(txt(th.fg(C::Muted, "Task: ") + &th.fg(C::Dim, text(&r["task"]))));
            expanded_output(&mut c, r, th, false);
            let stats = usage(&r["usage"], r["model"].as_str());
            if !stats.is_empty() {
                c.add_child(txt(th.fg(C::Dim, &stats)));
            }
        }
        if !stats.is_empty() {
            spacer(&mut c);
            c.add_child(txt(th.fg(C::Dim, &format!("Total: {stats}"))));
        }
        return component(c);
    }
    let mut s = header;
    for r in rs {
        let ok = if chain { r["exitCode"] == 0 } else { !failed(r) };
        let icon = th.fg(if ok { C::Success } else { C::Error }, if ok { "✓" } else { "✗" });
        let prefix = if chain {
            format!("─── Step {}: ", r["step"])
        } else {
            "─── ".into()
        };
        s += &format!(
            "\n\n{}{} {icon}",
            th.fg(C::Muted, &prefix),
            th.fg(C::Accent, text(&r["agent"]))
        );
        let display_items = items(r);
        if display_items.is_empty() {
            s += &format!(
                "\n{}",
                th.fg(
                    C::Muted,
                    if !chain && r["exitCode"] == -1 {
                        "(running...)"
                    } else {
                        "(no output)"
                    }
                )
            );
        } else {
            s += &format!("\n{}", display(&display_items, 5, options.expanded, th));
        }
    }
    if running == 0 && !stats.is_empty() {
        s += &format!("\n\n{}", th.fg(C::Dim, &format!("Total: {stats}")));
    }
    if chain || !options.expanded {
        s += &format!("\n{}", th.fg(C::Muted, "(Ctrl+O to expand)"));
    }
    txt(s)
}
