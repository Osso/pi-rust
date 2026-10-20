import {createInterface} from 'node:readline';
import cp from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
import {resolve,dirname} from 'node:path';
import {pathToFileURL} from 'node:url';
// Redirect only process execution; the unchanged example assembles every argument,
// parses actual child pipes, accumulates usage and performs its own cleanup.
const spawn=cp.spawn;
cp.spawn=((_command:any,args:any,options:any)=>spawn('python3',[process.env.SUBAGENT_FIXTURE!,...args],options)) as any;
syncBuiltinESMExports();
async function main(){
 const file=process.argv[2],root=resolve(dirname(file),'../../../src');
 const load=(p:string)=>import(pathToFileURL(resolve(root,p)).href);
 const [{default:register},{createExtensionRuntime,loadExtensionFromFactory},{ExtensionRunner},{SessionManager},{createEventBus}]=await Promise.all([import(pathToFileURL(file).href),load('core/extensions/loader.ts'),load('core/extensions/runner.ts'),load('core/session-manager.ts'),load('core/event-bus.ts')]);
 for await(const line of createInterface({input:process.stdin})){
  const req=JSON.parse(line),runtime=createExtensionRuntime();
  const extension=await loadExtensionFromFactory(register,req.cwd,createEventBus(),runtime,'<example:subagent>');
  const runner=new ExtensionRunner([extension],runtime,req.cwd,SessionManager.inMemory(req.cwd),{} as any);
  runner.bindCore({refreshTools:()=>{}} as any,{getModel:()=>({provider:'fixture',id:'inherited'}),getThinkingLevel:()=> 'high'} as any);
  const confirmations:any[]=[];runner.setUIContext({confirm:async(title:string,message:string)=>{confirmations.push([title,message]);return req.confirm;}} as any,'tui');
  if(req.renders){
   const tool=runner.getToolDefinition('subagent')!;
   const theme={fg:(_r:string,t:string)=>t,bold:(t:string)=>t};
   const clean=(s:string)=>s.replace(/\x1b\[[0-?]*[ -/]*[@-~]/g,'');
   const renders=req.renders.map((r:any)=>{const c=r.result?tool.renderResult!(r.result,{expanded:r.expanded,isPartial:false},theme as any,{} as any):tool.renderCall!(r.args,theme as any,{} as any);return c.render(100).map(clean);});
   console.log(JSON.stringify({renders}));continue;
  }
  let updateCount=0;
  const result=await runner.getToolDefinition('subagent')!.execute('call',req.params,undefined,()=>{updateCount++;},runner.createContext());
  console.log(JSON.stringify({result,confirmations,updateCount}));
 }
}
main().catch(e=>{console.error(e);process.exit(1)});
