// Real unchanged extension, loader and built-in tool implementations.
import { pathToFileURL } from "node:url";
import { resolve, dirname } from "node:path";
async function main() {
 const file=process.argv[2], packages=resolve(dirname(file),"../../..");
 const load=(path:string)=>import(pathToFileURL(resolve(packages,path)).href);
 const [{default:factory},loader,runnerModule,session,events,themeModule]=await Promise.all([
  import(pathToFileURL(file).href),load("coding-agent/src/core/extensions/loader.ts"),load("coding-agent/src/core/extensions/runner.ts"),load("coding-agent/src/core/session-manager.ts"),load("coding-agent/src/core/event-bus.ts"),load("coding-agent/src/modes/interactive/theme/theme.ts")]);
 themeModule.initTheme("dark");
 const theme=themeModule.theme;
 async function run(request:any) {
  const runtime=loader.createExtensionRuntime();
  const extension=await loader.loadExtensionFromFactory(factory,request.cwd,events.createEventBus(),runtime,file);
  const runner=new runnerModule.ExtensionRunner([extension],runtime,request.cwd,session.SessionManager.inMemory(request.cwd),{} as any);
  runner.bindCore({} as any,{} as any);
  const metadata=[...extension.tools.values()].map(({definition:d}:any)=>({name:d.name,label:d.label,description:d.description,parameters:d.parameters}));
  const calls=request.calls.map((item:any)=>{
   const tool=extension.tools.get(item.name).definition;
   const context={cwd:request.cwd};
   return {call:tool.renderCall(item.args,theme,context).render(1000),result:tool.renderResult(item.result,{expanded:item.expanded},theme,context).render(1000)};
  });
  const executions=[];
  for (const item of request.executions??[]) {
   const tool=extension.tools.get(item.name).definition;
   try { executions.push({value:await tool.execute("fixture",item.args,undefined,undefined,runner.createContext())}); }
   catch(error:any) { executions.push({error:error.message}); }
  }
  return {metadata,calls,executions};
 }
 process.stdin.setEncoding("utf8");let pending="";
 for await(const chunk of process.stdin){pending+=chunk;let end:number;while((end=pending.indexOf("\n"))!==-1){const request=JSON.parse(pending.slice(0,end));pending=pending.slice(end+1);console.log(JSON.stringify(await run(request)));}}
 if(pending)throw new Error("Unterminated request");
}
main().catch(error=>{console.error(error);process.exitCode=1;});
