// Batch G extends A/B's real loader/runner fixture pattern with mode state/actions.
import { resolve, dirname } from "node:path";
import { pathToFileURL } from "node:url";
async function main() {
 const source=process.argv[2];
 const base=source.endsWith("index.ts")?resolve(dirname(source),"../../../src"):resolve(dirname(source),"../../src");
 const load=(p:string)=>import(pathToFileURL(resolve(base,p)).href);
 const [{default:factory},loader,runnerModule,session,events,auth,utils,themeModule]=await Promise.all([
  import(pathToFileURL(source).href),load("core/extensions/loader.ts"),load("core/extensions/runner.ts"),load("core/session-manager.ts"),load("core/event-bus.ts"),load("core/auth-storage.ts"),load("core/model-registry.ts"),load("modes/interactive/theme/theme.ts")]);
 themeModule.initTheme("dark");
 async function run(request:any) {
  const cwd=request.cwd??"/fixture",trace:any[]=[];
  const originalError=console.error;console.error=(...args:any[])=>trace.push(["logError",args.join(" ")]);
  const runtime=loader.createExtensionRuntime();
  const extension=await loader.loadExtensionFromFactory(factory,cwd,events.createEventBus(),runtime,"<example:g>");
  const manager=session.SessionManager.inMemory(cwd);
  const registry=utils.ModelRegistry.inMemory(auth.AuthStorage.inMemory());
  if(request.models)registry.registerProvider("fixture",{name:"fixture",api:"openai-completions",baseUrl:"https://fixture.invalid/v1",apiKey:"fixture-key",models:request.models});
  let tools=request.tools??["read","bash","edit","write","custom"],thinking=request.thinking??"off",model:any=request.initialModel;
  const runner=new runnerModule.ExtensionRunner([extension],runtime,cwd,manager,registry);
  const allTools=["read","bash","edit","write","grep","find","ls","ask_questions","custom"].map(name=>({name,description:name,parameters:{}}));
  runner.bindCore({
   getActiveTools:()=>tools,setActiveTools:(value:string[])=>{tools=value;trace.push(["tools",value]);},getAllTools:()=>allTools,
   getThinkingLevel:()=>thinking,setThinkingLevel:(value:string)=>{thinking=value;trace.push(["thinking",value]);},
   setModel:async(value:any)=>{trace.push(["model",value]);if(request.modelSuccess!==false)model=value;return request.modelSuccess!==false;},
   appendEntry:(customType:string,data:any)=>{manager.appendCustomEntry(customType,request.fixed?structuredClone(data):data);trace.push(["append",customType,structuredClone(data)]);},
   sendMessage:(message:any,options:any)=>trace.push(["message",message,options??null]),
   sendUserMessage:(message:any,options:any)=>trace.push(["user",message,options??null]),
  } as any,{getModel:()=>model,getSystemPrompt:()=>"base",getSignal:()=>undefined} as any);
  const metadata={commands:[...extension.commands.values()].map((c:any)=>[c.name,c.description,!!c.getArgumentCompletions]),flags:[...extension.flags.values()].map((f:any)=>[f.name,f.description,f.type,f.default??null]),shortcuts:[...extension.shortcuts.values()].map((s:any)=>[s.shortcut,s.description])};
  runner.onError((error:any)=>trace.push(["handlerError",error.event,error.error]));
  for(const [key,value] of Object.entries(request.flags??{})) runner.setFlagValue(key,value);
  for(const item of request.entries??[]) { if(item.type==="custom")manager.appendCustomEntry(item.customType,item.data);else if(item.type==="custom_message")manager.appendCustomMessageEntry(item.customType,item.content,item.display,item.details);else manager.appendMessage(item.message); }
  for(const step of request.steps) {
   runner.setUIContext(step.hasUI===false?undefined:{
    theme:themeModule.theme,
    notify:(text:string,level?:string)=>trace.push(["notify",text,level??null]),
    setStatus:(key:string,text?:string)=>trace.push(["status",key,text??null]),
    setWidget:(key:string,text?:string[],options?:any)=>trace.push(["widget",key,text??null,options??null]),
    select:async(title:string,options:string[])=>{trace.push(["select",title,options]);return step.selected??undefined;},
    editor:async(title:string,prefill:string)=>{trace.push(["editor",title,prefill]);return step.editor??undefined;},
    custom:async(factory:any)=>{let result:any;const component=factory({requestRender:()=>{}},themeModule.theme,{},(value:any)=>result=value);trace.push(["custom",component.render(80)]);for(const key of step.keys??["\u001b"]){component.handleInput(key);if(result===undefined)trace.push(["customRender",component.render(80)]);}return result;},
   } as any,"tui");
   switch(step.op) {
    case "command":try{await runner.getCommand(step.name).handler(step.args??"",runner.createCommandContext());}catch(error:any){trace.push(["error",error.message]);}break;
    case "shortcut":await extension.shortcuts.get(step.key).handler(runner.createContext());break;
    case "event":await runner.emit(step.event);break;
    case "before":trace.push(["before",await runner.emitBeforeAgentStart("task",undefined,step.prompt??"base",{})??null]);break;
    case "context":trace.push(["context",await runner.emitContext(step.messages)]);break;
    case "tool":trace.push(["tool",await runner.emitToolCall({type:"tool_call",toolName:step.name,toolCallId:"fixture",input:step.input,bypassPermissions:false})??null]);break;
    case "tools":tools=step.value;break;
    default:throw new Error(`Unknown op ${step.op}`);
   }
  }
  const entries=manager.getEntries().map(({id,parentId,timestamp,...entry}:any)=>entry);
  console.error=originalError;return {metadata,trace,tools,thinking,model:model??null,entries};
 }
 process.stdin.setEncoding("utf8");let pending="";
 for await(const chunk of process.stdin){pending+=chunk;let end:number;while((end=pending.indexOf("\n"))!==-1){const request=JSON.parse(pending.slice(0,end));pending=pending.slice(end+1);const output=await run(request);console.log(JSON.stringify(output));}}
 if(pending)throw new Error("Unterminated request");
}
main().catch(error=>{console.error(error);process.exitCode=1;});
