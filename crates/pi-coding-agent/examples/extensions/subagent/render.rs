use super::{failed, output, text};
use pi_agent_core::AgentToolResult;
use pi_coding_agent::{
    core::extensions::{ToolRenderContext, ToolRenderResultOptions},
    modes::interactive::theme::{Theme, ThemeColor as C, get_markdown_theme},
};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::JsValue as Value;
use pi_tui::{
    Markdown, Spacer, Text,
    tui::{ComponentRef, Container, component},
};
fn txt(s: impl Into<JsString>) -> ComponentRef {
    component(Text::new(s, 0, 0))
}
fn preview(s: &JsString, n: usize) -> JsString {
    if s.len() > n {
        js_format!("{}...", s.slice(0, n))
    } else {
        s.clone()
    }
}

fn number(n: f64) -> JsString {
    JsString::from(pi_js_compat::number::js_number_to_string(n))
}
fn tokens(n: f64) -> JsString {
    if n < 1000.0 {
        number(n)
    } else if n < 10000.0 {
        js_format!("{}k", pi_js_compat::number::to_fixed(n / 1000.0, 1))
    } else if n < 1000000.0 {
        js_format!("{}k", number((n / 1000.0 + 0.5).floor()))
    } else {
        js_format!("{}M", pi_js_compat::number::to_fixed(n / 1000000.0, 1))
    }
}
fn usage(v: &JsValue, model: Option<&JsString>) -> JsString {
    let mut parts = vec![];
    let turns = v["turns"].as_f64().unwrap_or(0.0);
    if turns != 0.0 {
        parts.push(js_format!(
            "{} turn{}",
            number(turns),
            if turns > 1.0 { "s" } else { "" }
        ));
    }
    for (key, prefix) in [("input", "↑"), ("output", "↓"), ("cacheRead", "R"), ("cacheWrite", "W")] {
        let n = v[key].as_f64().unwrap_or(0.0);
        if n != 0.0 {
            parts.push(js_format!("{}{}", prefix, tokens(n)));
        }
    }
    let cost = v["cost"].as_f64().unwrap_or(0.0);
    if cost != 0.0 {
        parts.push(js_format!("${}", pi_js_compat::number::to_fixed(cost, 4)));
    }
    let context = v["contextTokens"].as_f64().unwrap_or(0.0);
    if context > 0.0 {
        parts.push(js_format!("ctx:{}", tokens(context)));
    }
    if let Some(m) = model.filter(|m| !m.is_empty()) {
        parts.push(m.into());
    }
    JsString::join(parts, " ")
}
pub fn call(args: &Value, th: &Theme, _: &ToolRenderContext) -> ComponentRef {
    let scope = args["agentScope"]
        .as_string()
        .cloned()
        .unwrap_or_else(|| JsString::from("user"));
    for (key, label) in [("chain", "steps"), ("tasks", "tasks")] {
        if let Some(items) = args[key].as_array().filter(|a| !a.is_empty()) {
            let mode = if key == "chain" { "chain" } else { "parallel" };
            let mut s = th.fg(C::ToolTitle, th.bold("subagent "))
                + &th.fg(C::Accent, &js_format!("{} ({} {})", mode, items.len(), label))
                + &th.fg(C::Muted, &js_format!(" [{}]", scope));
            for (i, item) in items.iter().take(3).enumerate() {
                let task = if key == "chain" {
                    text(&item["task"]).replace("{previous}", "").trim()
                } else {
                    text(&item["task"])
                };
                s.push_str("\n  ");
                if key == "chain" {
                    s.push_str(&(th.fg(C::Muted, &js_format!("{}.", i + 1)) + " "));
                }
                s.push_str(
                    &(th.fg(C::Accent, text(&item["agent"]))
                        + &th.fg(C::Dim, &js_format!(" {}", preview(&JsString::from(&task), 40)))),
                );
            }
            if items.len() > 3 {
                s.push_str(&js_format!(
                    "\n  {}",
                    th.fg(C::Muted, &js_format!("... +{} more", items.len() - 3))
                ));
            }
            return txt(s);
        }
    }
    let agent = args["agent"]
        .as_string()
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| JsString::from("..."));
    let task = args["task"]
        .as_string()
        .filter(|s| !s.is_empty())
        .map(|s| preview(s, 60))
        .unwrap_or_else(|| "...".into());
    txt(th.fg(C::ToolTitle, th.bold("subagent "))
        + &th.fg(C::Accent, agent)
        + &th.fg(C::Muted, &js_format!(" [{}]", scope))
        + &js_format!("\n  {}", th.fg(C::Dim, &task)))
}
fn items(r: &JsValue) -> Vec<Value> {
    r["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["role"] == pi_js_compat::js_json!("assistant"))
        .flat_map(|m| m["content"].as_array().into_iter().flatten())
        .filter(|p| p["type"] == pi_js_compat::js_json!("text") || p["type"] == pi_js_compat::js_json!("toolCall"))
        .cloned()
        .collect()
}
fn tool(p: &Value, th: &Theme) -> JsString {
    let name = text(&p["name"]);
    let a = &p["arguments"];
    let mut path = a["file_path"]
        .as_string()
        .filter(|s| !s.is_empty())
        .or_else(|| a["path"].as_string().filter(|s| !s.is_empty()))
        .cloned()
        .unwrap_or_else(|| {
            JsString::from(if name == "find" || name == "grep" || name == "ls" {
                "."
            } else {
                "..."
            })
        });
    if let Some(home) = std::env::var_os("HOME") {
        let home = JsString::from(home.to_str().expect("home directory must be UTF-8"));
        if path.starts_with(&home) {
            path = js_format!("~{}", path.slice(home.len(), path.len()));
        }
    }
    match name {
        name if name == "bash" => {
            th.fg(C::Muted, "$ ")
                + &th.fg(
                    C::ToolOutput,
                    preview(
                        &a["command"]
                            .as_string()
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .unwrap_or_else(|| JsString::from("...")),
                        60,
                    ),
                )
        }
        name if name == "read" => {
            let mut s = th.fg(C::Muted, "read ") + &th.fg(C::Accent, &path);
            if a.get("offset").is_some() || a.get("limit").is_some() {
                let start = a["offset"].as_f64().unwrap_or(1.0);
                let end = a["limit"]
                    .as_f64()
                    .map(|n| start + n - 1.0)
                    .filter(|n| *n != 0.0)
                    .map(|n| js_format!("-{}", pi_js_compat::number::js_number_to_string(n)))
                    .unwrap_or_default();
                s.push_str(th.fg(
                    C::Warning,
                    &js_format!(":{}{}", pi_js_compat::number::js_number_to_string(start), end),
                ));
            }
            s
        }
        name if name == "write" => {
            let n = text(&a["content"]).split('\n').count();
            let mut s = th.fg(C::Muted, "write ") + &th.fg(C::Accent, &path);
            if n > 1 {
                s.push_str(th.fg(C::Dim, &js_format!(" ({} lines)", n)));
            }
            s
        }
        name if name == "edit" || name == "ls" => th.fg(C::Muted, &js_format!("{} ", name)) + &th.fg(C::Accent, &path),
        name if name == "find" => {
            th.fg(C::Muted, "find ")
                + &th.fg(
                    C::Accent,
                    a["pattern"]
                        .as_string()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&JsString::from("*")),
                )
                + &th.fg(C::Dim, &js_format!(" in {}", path))
        }
        name if name == "grep" => {
            th.fg(C::Muted, "grep ")
                + &th.fg(C::Accent, &js_format!("/{}/", text(&a["pattern"])))
                + &th.fg(C::Dim, &js_format!(" in {}", path))
        }
        _ => {
            th.fg(C::Accent, name)
                + &th.fg(
                    C::Dim,
                    &js_format!(
                        " {}",
                        preview(&JsString::from(pi_js_compat::js_json::stringify_lossless(a)), 50)
                    ),
                )
        }
    }
}
fn display(items: &[Value], limit: usize, expanded: bool, th: &Theme) -> JsString {
    let skip = items.len().saturating_sub(limit);
    let mut s = JsString::default();
    if skip > 0 {
        s.push_str(th.fg(C::Muted, &js_format!("... {} earlier items\n", skip)));
    }
    for item in &items[skip..] {
        if item["type"] == pi_js_compat::js_json!("text") {
            let t = text(&item["text"]);
            let t = if expanded {
                t
            } else {
                JsString::join(t.split('\n').take(3), "\n")
            };
            s.push_str(th.fg(C::ToolOutput, &t));
        } else {
            s.push_str(&(th.fg(C::Muted, "→ ") + &tool(item, th)));
        }
        s.push_str("\n");
    }
    s.trim_end()
}
fn spacer(c: &mut Container) {
    c.add_child(component(Spacer::new(1)));
}
fn expanded_output(c: &mut Container, r: &JsValue, th: &Theme, single: bool) {
    let display = items(r);
    let final_output = output(&r["messages"]);
    if single && display.is_empty() && final_output.is_empty() {
        c.add_child(txt(th.fg(C::Muted, "(no output)")));
        return;
    }
    for item in &display {
        if item["type"] == pi_js_compat::js_json!("toolCall") {
            c.add_child(txt(th.fg(C::Muted, "→ ") + &tool(item, th)));
        }
    }
    if !final_output.is_empty() {
        spacer(c);
        c.add_child(component(Markdown::new(
            final_output.trim(),
            0,
            0,
            get_markdown_theme(),
            None,
            Default::default(),
        )));
    }
}
fn aggregate(rs: &[Value]) -> Value {
    let mut v = pi_js_compat::js_json!({"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"cost":0,"turns":0});
    for r in rs {
        for key in ["input", "output", "cacheRead", "cacheWrite", "cost", "turns"] {
            v[key] = pi_js_compat::js_json!(v[key].as_f64().unwrap_or(0.0) + r["usage"][key].as_f64().unwrap_or(0.0));
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
    if d["mode"] == pi_js_compat::js_json!("single") && rs.len() == 1 {
        let r = &rs[0];
        let error = failed(r);
        let icon = th.fg(if error { C::Error } else { C::Success }, if error { "✗" } else { "✓" });
        let mut header = icon
            + " "
            + &th.fg(C::ToolTitle, th.bold(text(&r["agent"])))
            + &th.fg(C::Muted, &js_format!(" ({})", text(&r["agentSource"])));
        if error && r["stopReason"].as_string().is_some_and(|s| !s.is_empty()) {
            header.push_str(&js_format!(
                " {}",
                th.fg(C::Error, &js_format!("[{}]", text(&r["stopReason"])))
            ));
        }
        let stats = usage(&r["usage"], r["model"].as_string());
        if options.expanded {
            let mut c = Container::new();
            c.add_child(txt(header));
            if error && r["errorMessage"].as_string().is_some_and(|s| !s.is_empty()) {
                c.add_child(txt(th.fg(C::Error, &js_format!("Error: {}", text(&r["errorMessage"])))));
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
        if error && r["errorMessage"].as_string().is_some_and(|s| !s.is_empty()) {
            header.push_str(&js_format!(
                "\n{}",
                th.fg(C::Error, &js_format!("Error: {}", text(&r["errorMessage"])))
            ));
        } else if display_items.is_empty() {
            header.push_str(&js_format!("\n{}", th.fg(C::Muted, "(no output)")));
        } else {
            header.push_str(&js_format!("\n{}", display(&display_items, 10, false, th)));
            if display_items.len() > 10 {
                header.push_str(&js_format!("\n{}", th.fg(C::Muted, "(Ctrl+O to expand)")));
            }
        }
        if !stats.is_empty() {
            header.push_str(&js_format!("\n{}", th.fg(C::Dim, &stats)));
        }
        return txt(header);
    }
    let chain = d["mode"] == pi_js_compat::js_json!("chain");
    if !chain && d["mode"] != pi_js_compat::js_json!("parallel") {
        return fallback();
    }
    let running = if chain {
        0
    } else {
        rs.iter()
            .filter(|r| r["exitCode"] == pi_js_compat::js_json!(-1))
            .count()
    };
    let success = rs
        .iter()
        .filter(|r| {
            if chain {
                r["exitCode"] == pi_js_compat::js_json!(0)
            } else {
                r["exitCode"] != pi_js_compat::js_json!(-1) && !failed(r)
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
        js_format!("{}/{} steps", success, rs.len())
    } else if running > 0 {
        js_format!("{}/{} done, {} running", success + fail, rs.len(), running)
    } else {
        js_format!("{}/{} tasks", success, rs.len())
    };
    let header = th.fg(role, icon)
        + " "
        + &th.fg(C::ToolTitle, th.bold(if chain { "chain " } else { "parallel " }))
        + &th.fg(C::Accent, &status);
    let stats = usage(&aggregate(rs), None);
    if options.expanded && running == 0 {
        let mut c = Container::new();
        c.add_child(txt(header));
        for r in rs {
            spacer(&mut c);
            let ok = if chain {
                r["exitCode"] == pi_js_compat::js_json!(0)
            } else {
                !failed(r)
            };
            let icon = th.fg(if ok { C::Success } else { C::Error }, if ok { "✓" } else { "✗" });
            let prefix = if chain {
                js_format!(
                    "─── Step {}: ",
                    pi_js_compat::js_json::to_js_string(&r["step"]).expect("step value converts to JS text")
                )
            } else {
                "─── ".into()
            };
            let heading = th.fg(C::Muted, &prefix) + &th.fg(C::Accent, text(&r["agent"])) + " " + &icon;
            c.add_child(txt(heading));
            c.add_child(txt(th.fg(C::Muted, "Task: ") + &th.fg(C::Dim, text(&r["task"]))));
            expanded_output(&mut c, r, th, false);
            let stats = usage(&r["usage"], r["model"].as_string());
            if !stats.is_empty() {
                c.add_child(txt(th.fg(C::Dim, &stats)));
            }
        }
        if !stats.is_empty() {
            spacer(&mut c);
            c.add_child(txt(th.fg(C::Dim, &js_format!("Total: {}", stats))));
        }
        return component(c);
    }
    let mut s = header;
    for r in rs {
        let ok = if chain {
            r["exitCode"] == pi_js_compat::js_json!(0)
        } else {
            !failed(r)
        };
        let icon = if !chain && r["exitCode"] == pi_js_compat::js_json!(-1) {
            th.fg(C::Warning, "⏳")
        } else {
            th.fg(if ok { C::Success } else { C::Error }, if ok { "✓" } else { "✗" })
        };
        let prefix = if chain {
            js_format!(
                "─── Step {}: ",
                pi_js_compat::js_json::to_js_string(&r["step"]).expect("step value converts to JS text")
            )
        } else {
            "─── ".into()
        };
        s.push_str(&js_format!(
            "\n\n{}{} {}",
            th.fg(C::Muted, &prefix),
            th.fg(C::Accent, text(&r["agent"])),
            icon
        ));
        let display_items = items(r);
        if display_items.is_empty() {
            s.push_str(&js_format!(
                "\n{}",
                th.fg(
                    C::Muted,
                    if !chain && r["exitCode"] == pi_js_compat::js_json!(-1) {
                        "(running...)"
                    } else {
                        "(no output)"
                    }
                )
            ));
        } else {
            s.push_str(&js_format!("\n{}", display(&display_items, 5, options.expanded, th)));
        }
    }
    if running == 0 && !stats.is_empty() {
        s.push_str(&js_format!("\n\n{}", th.fg(C::Dim, &js_format!("Total: {}", stats))));
    }
    if chain || !options.expanded {
        s.push_str(&js_format!("\n{}", th.fg(C::Muted, "(Ctrl+O to expand)")));
    }
    txt(s)
}
