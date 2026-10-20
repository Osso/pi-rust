// Real pinned RPC mode, private SDK services, explicit extension factory only.
import { pathToFileURL } from "node:url";
import { resolve, dirname } from "node:path";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdirSync } from "node:fs";
const source=process.argv[2];
async function child() {
 const cwd=process.argv[4], packages=resolve(dirname(source),"../../..");
 const load=(path: string)=>import(pathToFileURL(resolve(packages,path)).href);
 const [{default:factory},loader,events,sdk,session,models,auth,settings,runtimeModule,rpc]=await Promise.all([
  import(pathToFileURL(source).href),load("coding-agent/src/core/extensions/loader.ts"),load("coding-agent/src/core/event-bus.ts"),load("coding-agent/src/core/sdk.ts"),load("coding-agent/src/core/session-manager.ts"),load("coding-agent/src/core/model-runtime.ts"),load("coding-agent/src/core/auth-storage.ts"),load("coding-agent/src/core/settings-manager.ts"),load("coding-agent/src/core/agent-session-runtime.ts"),load("coding-agent/src/modes/rpc/rpc-mode.ts")
 ]);
 let active: any;
 const runtime=loader.createExtensionRuntime();
 const extension=await loader.loadExtensionFromFactory((pi: any)=>{
  factory(pi);
  pi.registerCommand("fixture-event",{description:"Private RPC event driver",handler:async(args: string,ctx: any)=>{
   const event=JSON.parse(args),runner=active.extensionRunner,ui=ctx.ui;
   if(event.noUi) runner.setUIContext(undefined,"rpc");
   const result=event.type==="tool_call" ? await runner.emitToolCall(event) : await runner.emit(event);
   runner.setUIContext(ui,"rpc");
   ui.notify(`fixture result: ${JSON.stringify(result ?? null)}`,"info");
  }});
 },cwd,events.createEventBus(),runtime,"/fixture/rpc-demo");
 const resourceLoader={
  getExtensions:()=>({extensions:[extension],errors:[],runtime}),getSkills:()=>({skills:[],diagnostics:[]}),getPrompts:()=>({prompts:[],diagnostics:[]}),getThemes:()=>({themes:[],diagnostics:[]}),getAgentsFiles:()=>({agentsFiles:[]}),getRulesFiles:()=>({rulesFiles:[]}),getRulesContent:()=>undefined,getSystemPrompt:()=>undefined,getSystemPromptSource:()=>undefined,getAppendSystemPrompt:()=>[],getAppendSystemPromptSources:()=>[],extendResources(){},reload:async()=>{}
 };
 const agentDir=resolve(cwd,"ts-agent");mkdirSync(agentDir,{recursive:true});
 const modelRuntime=await models.ModelRuntime.create({credentials:auth.AuthStorage.inMemory(),modelsPath:null,allowModelNetwork:false});
 const settingsManager=settings.SettingsManager.inMemory({});
 const services={cwd,agentDir,modelRuntime,settingsManager,resourceLoader,diagnostics:[]};
 const create=async(manager: any,reason: string)=>{
  const result=await sdk.createAgentSession({cwd,agentDir,modelRuntime,settingsManager,resourceLoader,sessionManager:manager,tools:[],disableRuntimeCoordinationInbound:true,sessionStartEvent:{type:"session_start",reason}});
  active=result.session; return {...result,services,diagnostics:[]};
 };
 const initial=await create(session.SessionManager.inMemory(cwd),"startup");
 const host=new runtimeModule.AgentSessionRuntime(initial.session,services,async(options: any)=>create(options.sessionManager,"new"));
 await rpc.runRpcMode(host);
}
async function run(request: any) {
 const child=spawn(process.execPath,[...process.execArgv,process.argv[1],source,"--child",request.cwd],{stdio:["pipe","pipe","inherit"]});
 const frames: any[]=[];
 const lines=createInterface({input:child.stdout});
 const iterator=lines[Symbol.asyncIterator]();
 const next=async()=>{
  let timer: ReturnType<typeof setTimeout>;
  const result=await Promise.race([iterator.next(),new Promise<never>((_,reject)=>{timer=setTimeout(()=>reject(new Error("RPC frame timeout")),20000);})]).finally(()=>clearTimeout(timer));
  if(result.done) throw new Error("RPC child exited early");
  const frame=JSON.parse(result.value);frames.push(frame);return frame;
 };
 try {
  while((await next()).method!=="setStatus") {}
  for(const step of request.scripts) {
   child.stdin.write(JSON.stringify(step.command)+"\n");
   while(true) {
    const frame=await next();
    if(frame.type==="extension_ui_request" && ["input","editor","select","confirm"].includes(frame.method)) child.stdin.write(JSON.stringify({...step.response,type:"extension_ui_response",id:frame.id})+"\n");
    if(frame.type==="response" && frame.id===step.command.id) break;
   }
  }
  child.stdin.end();
  const code=await new Promise<number|null>(done=>child.on("exit",done));
  if(code!==0) throw new Error(`RPC child exit ${code}`);
  return frames;
 } finally {child.kill("SIGKILL");lines.close();}
}
async function main(){
 if(process.argv[3]==="--child"){await child();return;}
 for await(const line of createInterface({input:process.stdin})) console.log(JSON.stringify(await run(JSON.parse(line))));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
