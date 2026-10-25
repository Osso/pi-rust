# HTML-export asset modules

Source paths below are relative to `packages/coding-agent/src/core/export-html/` at the [original pin](../source-lock.json). Rust paths are relative to `crates/pi-coding-agent-tools/src/core/export_html/templates/`. Full byte arrays compared equal on 2026-10-20 at the [proof-ledger revision](../proven-assets-proof.md).

| Source = Rust relative path | Bytes | SHA-256 | Inclusion constant |
|---|---:|---|---|
| `template.css` | 22724 | `28c16e3827c23a62eef8283cac316b478f946e023093adad25ff9c9b891d41af` | `TEMPLATE_CSS` |
| `template.html` | 2225 | `916782b1184a9597527605ad751e2b3af30fcea23ba2194002969cd217a06881` | `TEMPLATE_HTML` |
| `template.js` | 76742 | `1893cdb77587f592eef5717905391269886d6d7a4dc6a488417a73da374d9226` | `TEMPLATE_JS` |
| `vendor/highlight.min.js` | 121727 | `837a6fa5b0c736b52bbde2b2b6190f305da3fc9ed41681db5321507057b5c846` | `HIGHLIGHT_JS` |
| `vendor/marked.min.js` | 42882 | `d5487edc7258b404bfa74c393d74a6393155f02517bd5e7e77cd64f8187f39a0` | `MARKED_JS` |

Each complete file is compiled into `crates/pi-coding-agent-tools/src/core/export_html/index.rs:19–23` with `include_str!`. `generate_html` at lines 309–318 uses every constant: theme substitutions in CSS, then HTML CSS/JS/session-data/vendor substitutions. Public `export_from_file` and `export_session_to_html` write that generated HTML; these are not orphan mirrors.

These modules are shipped HTML/CSS/browser-JS data, not translated Rust APIs. Complete byte identity covers every declaration/branch as raw content; it does not prove browser execution. Existing named output proof: `differential_export_from_file_matches_byte_for_byte`, `exports_file_sessions_with_embedded_session_data`, and `exports_live_sessions_with_prompt_and_tools`; exact current pass locations and limitations live in the [proof ledger](../proven-assets-proof.md). Rust replacement semantics and export API branches remain mapped in [export traceability](coding-agent-export-html.md).

Status: all five asset rows `done`. No new exclusion.
