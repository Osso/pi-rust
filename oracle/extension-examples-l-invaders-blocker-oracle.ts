// Unchanged pinned game factories, real loader/runner, deterministic time/random.
import {dirname,resolve} from "node:path";
import {pathToFileURL} from "node:url";
import {createReadonlySessionManager} from "./extension-examples-l-readonly-session-manager.ts";
async function main(){
 const source=process.argv[2],base=resolve(dirname(source),"../../src");
 const load=(path:string)=>import(pathToFileURL(resolve(base,path)).href);
 const [{default:factory},loader,runnerModule,session,events,auth,utils,tuiModule,terminalModule]=await Promise.all([import(pathToFileURL(source).href),load("core/extensions/loader.ts"),load("core/extensions/runner.ts"),load("core/session-manager.ts"),load("core/event-bus.ts"),load("core/auth-storage.ts"),load("../test/model-runtime-test-utils.ts"),load("../../tui/src/tui.ts"),load("../../tui/test/virtual-terminal.ts")]);
 async function run(request:any){
  const trace:any[]=[],runtime=loader.createExtensionRuntime();
  const extension=await loader.loadExtensionFromFactory(factory,"/fixture",events.createEventBus(),runtime,"<example:l-game>");
  const manager=session.SessionManager.inMemory("/fixture");
  for(const entry of request.entries??[])manager.appendCustomEntry(entry.customType,entry.data);
  const registry=await utils.createInMemoryModelRegistry(auth.AuthStorage.inMemory());
  const runner=new runnerModule.ExtensionRunner([extension],runtime,"/fixture",manager,registry);
  if(request.fixed){const original=runner.createContext.bind(runner);runner.createContext=()=>({...original(),sessionManager:createReadonlySessionManager(manager)});}
  runner.bindCore({appendEntry:(kind:string,data:any)=>{manager.appendCustomEntry(kind,request.fixed?structuredClone(data):data);trace.push(["append",kind,structuredClone(data)]);}} as any,{getModel:()=>undefined,getSystemPrompt:()=>"",getSignal:()=>undefined} as any);
  const originalRandom=Math.random,originalSetInterval=globalThis.setInterval,originalClearInterval=globalThis.clearInterval;
  let randomIndex=0,now=0,nextId=0;const timers=new Map<number,{next:number,ms:number,fn:()=>void}>();
  Math.random=()=>{if(randomIndex>=request.random.length)throw new Error("random fixture exhausted");return request.random[randomIndex++];};
  globalThis.setInterval=((fn:()=>void,ms:number)=>{const id=++nextId;timers.set(id,{next:now+ms,ms,fn});return id;}) as any;
  globalThis.clearInterval=((id:number)=>{timers.delete(id);}) as any;
  const advance=(ms:number)=>{const until=now+ms;while(true){const next=[...timers].sort((a,b)=>a[1].next-b[1].next||a[0]-b[0])[0];if(!next||next[1].next>until)break;now=next[1].next;next[1].next+=next[1].ms;next[1].fn();}now=until;};
  try{
   for(const item of request.commands){
    runner.setUIContext({notify:(text:string,kind:string)=>trace.push(["notify",text,kind]),custom:async(factory:any)=>{
     let done=false;const component=factory({requestRender:()=>trace.push(["requestRender"])},{},{},()=>{done=true;trace.push(["done"]);});
     const terminal=new terminalModule.VirtualTerminal(90,40);const host=new tuiModule.TUI(terminal);host.addChild(component);host.setFocus(component);host.start();
     const render=()=>{for(const width of item.widths){let frame:any;try{frame=component.render(width);}catch(error:any){frame={error:`${error.name}: ${error.message}`};}trace.push(["render",width,frame]);}};render();
     for(const step of item.steps){if(step.op==="key")terminal.sendInput(step.key);else if(step.op==="ticks"){for(let i=0;i<step.count;i++)advance(step.ms??100);}else if(step.op==="invalidate")component.invalidate();else throw new Error("unknown fixture op");render();}
     if(!done)throw new Error("script must close custom component");component.dispose();host.stop();return undefined;
    }} as any,item.mode??"tui");
    await runner.getCommand(source.endsWith("/snake.ts")?"snake":"invaders").handler("",runner.createCommandContext());
   }
   if(timers.size)throw new Error("game leaked interval");
   return {metadata:[...extension.commands.values()].map((c:any)=>[c.name,c.description]),trace,entries:manager.getEntries().map(({id,parentId,timestamp,...e}:any)=>e)};
  }finally{Math.random=originalRandom;globalThis.setInterval=originalSetInterval;globalThis.clearInterval=originalClearInterval;}
 }
 process.stdin.setEncoding("utf8");let pending="";
 for await(const chunk of process.stdin){pending+=chunk;let end:number;while((end=pending.indexOf("\n"))!==-1){const request=JSON.parse(pending.slice(0,end));pending=pending.slice(end+1);console.log(JSON.stringify(await run(request)));}}
 if(pending)throw new Error("unterminated request");
}
main().catch(error=>{console.error(error);process.exitCode=1;});
