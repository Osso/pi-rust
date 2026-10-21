use super::*;
use tokio::io::AsyncReadExt;
use pi_tui::utils::{truncate_to_width,truncate_to_width_with};
#[derive(Default)]
struct Output{lines:Vec<String>,scroll:usize,finished:bool,disposed:bool}
pub(super) struct Process{output:Rc<RefCell<Output>>,pid:u32,readers:Vec<tokio::task::JoinHandle<()>>}
impl Process{
 pub(super) fn start(model:&Rc<Model>)->Self{
  let mut child=tokio::process::Command::new("bash").args(["-c",include_str!("overflow.sh.txt")]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().expect("overlay overflow: spawn bash");
  let pid=child.id().expect("spawned child pid");let output=Rc::new(RefCell::new(Output::default()));
  let stdout=child.stdout.take().expect("piped stdout");let stderr=child.stderr.take().expect("piped stderr");
  let readers=vec![read(stdout,false,output.clone(),model.theme.clone(),model.tui.clone()),read(stderr,true,output.clone(),model.theme.clone(),model.tui.clone())];
  let state=output.clone();let tui=model.tui.clone();
  tokio::task::spawn_local(async move{
   child.wait().await.expect("overlay overflow: reap subprocess");
   if !state.borrow().disposed{state.borrow_mut().finished=true;tui.request_render();}
  });
  Self{output,pid,readers}
 }
 fn kill(&self){
  if self.output.borrow().finished{return;}
  if let Some(pid)=rustix::process::Pid::from_raw(self.pid as i32){
   match rustix::process::kill_process(pid,rustix::process::Signal::TERM){Ok(())|Err(rustix::io::Errno::SRCH)=>{},Err(error)=>panic!("overlay overflow: terminate child: {error}")}
  }
 }
 pub(super) fn dispose(&mut self){self.output.borrow_mut().disposed=true;self.kill();for reader in self.readers.drain(..){reader.abort();}}
 pub(super) fn input(&self,data:&str){
  let mut state=self.output.borrow_mut();
  if matches_key(data,"up"){state.scroll=state.scroll.saturating_sub(1);}else if matches_key(data,"down"){state.scroll=(state.scroll+1).min(state.lines.len().saturating_sub(15));}
 }
 pub(super) fn render(&self,m:&Model,width:usize)->Vec<String>{
  let state=self.output.borrow();let inner=width.saturating_sub(2).max(1);let border=|s:&str|m.theme.fg(ThemeColor::Border,s);
  let title=truncate_to_width(&format!(" Streaming Output ({} lines) ",state.lines.len()),inner);
  let mut result=vec![format!("{}{}{}",border("╭"),m.theme.fg(ThemeColor::Accent,&title),border(&format!("{}╮","─".repeat(inner.saturating_sub(visible_width(&title))))))];
  let remaining=state.lines.len().saturating_sub(15+state.scroll);
  let scroll=if state.scroll>0||remaining>0{m.theme.fg(ThemeColor::Dim,&format!(" ↑{} | ↓{remaining}",state.scroll))}else{String::new()};
  let row=|s:&str|format!("{}{}{}",border("│"),truncate_to_width_with(s,inner,"...",true),border("│"));
  result.push(row(&scroll));
  let visible=state.lines.iter().skip(state.scroll).take(15).collect::<Vec<_>>();
  result.extend(visible.iter().map(|line|row(&format!(" {line}"))));
  result.extend((visible.len()..15).map(|_|row("")));
  let status=if state.finished{m.theme.fg(ThemeColor::Success,"✓ Done")}else{m.theme.fg(ThemeColor::Warning,"● Running")};
  result.push(row(&format!(" {status} {}",m.theme.fg(ThemeColor::Dim,"| ↑↓ scroll | Esc close"))));
  result.push(border(&format!("╰{}╯","─".repeat(inner))));result
 }
}
impl Drop for Process{fn drop(&mut self){self.dispose();}}
fn read(mut reader:impl tokio::io::AsyncRead+Unpin+'static,stderr:bool,output:Rc<RefCell<Output>>,theme:Arc<Theme>,tui:ExtensionUiTui)->tokio::task::JoinHandle<()>{
 tokio::task::spawn_local(async move{
  let mut buffer=[0;8192];
  loop{
   let length=reader.read(&mut buffer).await.expect("overlay overflow: read process output");if length==0{break;}
   if output.borrow().disposed{continue;}
   let text=String::from_utf8_lossy(&buffer[..length]);let mut state=output.borrow_mut();
   if stderr{state.lines.push(theme.fg(ThemeColor::Error,pi_js_compat::string::js_trim(&text)));}
   else{state.lines.extend(text.split('\n').filter(|line|!line.is_empty()).map(str::to_owned));state.scroll=state.lines.len().saturating_sub(15);}
   drop(state);tui.request_render();
  }
 })
}
