"""Build a standalone exact-module harness, not a replacement implementation.

SessionManager/tool-output, native paths/transcript/spool and core dependencies are
included unmodified. Metadata/resident listing/default configuration panic on use:
these boundaries are explicitly NOT covered. Worker binary executes actual spool code.
Run Cargo with CARGO_BUILD_JOBS=4 and Q18_HARNESS_PACKAGE_DIR=<target-dir>/debug.
This setup script executes no tests or other commands.
"""
from pathlib import Path
R=Path(__file__).resolve().parents[1]
H=Path('/tmp/pi-rust-repin-lossless-session-records/full/crates/pi-coding-agent')
H.mkdir(parents=True,exist_ok=True)
for name in ['docs','oracle']:
    link=H.parent.parent/name
    if link.is_symlink() and link.resolve() != (R/name).resolve(): link.unlink()
    if not link.exists(): link.symlink_to(R/name,target_is_directory=True)
if (H/'tests').is_symlink() and (H/'tests').resolve() != R/'crates/pi-coding-agent/tests': (H/'tests').unlink()
if not (H/'tests').exists(): (H/'tests').symlink_to(R/'crates/pi-coding-agent/tests',target_is_directory=True)
core=R/'crates/pi-coding-agent/src/core'
def struct(text,name):
    a=text.index('pub struct '+name)
    b=text.index('\n}',a)+2
    return '#[derive(Debug,Clone,PartialEq,serde::Serialize,serde::Deserialize)]\n'+text[a:b]+'\n'
def signature(text,name):
    a=text.index('pub fn '+name)
    b=text.index('{',a)
    import re
    signature=re.sub(r'(?<![\w:])([a-z_]+):',lambda m:'_'+m[1]+':',text[a:b])
    return signature+'{ panic!("metadata boundary excluded from harness") }\n'
sessions=(core/'session_control_db/sessions.rs').read_text()
mod=(core/'session_control_db/mod.rs').read_text()
names=['ResumeSessionMetadataOptions','SessionMetadata','WritableSessionMetadata','SessionNameState']
stubs='use pi_js_compat::js_json::JsString;\nuse super::permissions::presets::SandboxProfileName;\ntype ControlDbError=std::io::Error;\n'
for name in names: stubs+=struct(sessions,name)
stubs+=struct(mod,'RuntimeMailboxStoreRef')
functions=['clear_session_sandbox_profile','list_resume_session_metadata','list_session_metadata','read_session_goal','read_session_metadata','read_session_name_state','read_session_sandbox_profile','write_session_goal','write_session_metadata','write_session_model','write_session_name','write_session_sandbox_profile','write_session_thinking_level','relocate_session_control_data']
for name in functions: stubs+=signature(sessions,name)
a=sessions.index('pub fn normalize_session_name_utf16'); b=sessions.index('/// Scalar SQL metadata',a)
stubs+=sessions[a:b]
(H/'db.rs').write_text(stubs)
preset=(core/'permissions/presets.rs').read_text();a=preset.index('pub enum SandboxProfileName');b=preset.index('\n}',a)+2
(H/'presets.rs').write_text('#[derive(Debug,Clone,Copy,PartialEq,Eq)]\n'+preset[a:b])
modules=['session_manager','session_tool_output','session_transcript','session_transcript_spool']
coremods='\n'.join('#[path="'+str(core/(name+'.rs'))+'"] pub mod '+name+';' for name in modules)
lib='''pub mod config {
use std::path::PathBuf;
pub fn get_agent_dir()->PathBuf { panic!("default live config excluded") }
pub fn get_sessions_dir()->PathBuf { panic!("default live config excluded") }
pub fn get_package_dir()->PathBuf { std::env::var("Q18_HARNESS_PACKAGE_DIR").expect("private worker directory").into() }
}
pub mod utils {
#[path="'''+str(R/'crates/pi-coding-agent/src/utils/iso_time.rs')+'''"] pub mod iso_time;
#[path="'''+str(R/'crates/pi-coding-agent/src/utils/paths.rs')+'''"] pub mod paths;
}
pub mod core {
pub mod permissions { pub mod presets { include!("presets.rs"); } }
pub mod session_control_db { include!("db.rs"); }
pub mod multi_agent_store { pub trait SessionPersistence { fn metadata_control_db_path(&self)->Option<pi_js_compat::js_json::JsString>; fn session_file(&self)->Option<pi_js_compat::js_json::JsString>; } }
pub mod resident_session { pub fn is_resident_session(_: &super::session_manager::SessionInfo)->bool { panic!("resident listing excluded"); } }
pub mod session_archive_storage { pub fn is_archived_session_file(path:&str)->bool { path.ends_with(".zst") } }
'''+coremods+'\n}\n'
(H/'lib.rs').write_text(lib)
(H/'main.rs').write_text('fn main() { let args:Vec<String>=std::env::args().skip(1).collect(); let code=pi_coding_agent::core::session_transcript_spool::run_internal_session_transcript_worker(&args).expect("internal worker mode"); std::process::exit(code); }')
cargo='''[package]
name="q18-full-session-module-harness"
version="0.0.0"
edition="2024"
[lib]
name="pi_coding_agent"
path="lib.rs"
[[bin]]
name="pi"
path="main.rs"
'''
for filename in ['lossless_session_records','lossless_session_persistence','lossless_session_manager_io']:
 cargo+='[[test]]\nname="'+filename+'"\npath="'+str(R/'crates/pi-coding-agent/tests'/(filename+'.rs'))+'"\n'
cargo+='[dependencies]\n'
for name in ['pi-ai','pi-js-compat','pi-agent-core']: cargo+=name+'={path="'+str(R/'crates'/name)+'"}\n'
cargo+='serde={version="=1.0.229",features=["derive"]}\nserde_json={version="=1.0.151",features=["preserve_order","raw_value"]}\nrustix={version="=1.1.5",features=["process"]}\nurl="=2.5.8"\ntempfile="=3.27.0"\npercent-encoding="=2.3.2"\nzstd="=0.14.3"\n'
(H/'Cargo.toml').write_text(cargo)

print(H/"Cargo.toml")
