import {dirname,resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
async function main(){
 const source=process.argv[2],base=resolve(dirname(source),'../../src');
 const load=(p:string)=>import(pathToFileURL(resolve(base,p)).href);
 const [{default:factory},loader,runnerModule,events,session,auth,utils]=await Promise.all([import(pathToFileURL(source).href),load('core/extensions/loader.ts'),load('core/extensions/runner.ts'),load('core/event-bus.ts'),load('core/session-manager.ts'),load('core/auth-storage.ts'),load('core/model-registry.ts')]);
 async function run(request:any){
  if(request.projectUnits){const projected=String.fromCharCode(...request.projectUnits).toWellFormed();request.commands=[projected];request.env.INTERACTIVE_COMMANDS=projected;}
  const keys=['INTERACTIVE_COMMANDS','INTERACTIVE_EXCLUDE','SHELL'],previous=keys.map(k=>process.env[k]);
  for(const key of keys){if(request.env?.[key]===undefined)delete process.env[key];else process.env[key]=request.env?.[key];}
  try{
   const runtime=loader.createExtensionRuntime();
   const extension=await loader.loadExtensionFromFactory(factory,'/fixture',events.createEventBus(),runtime,'<example:interactive-shell>');
   const manager=session.SessionManager.inMemory('/fixture');
   const registry=utils.ModelRegistry.inMemory(auth.AuthStorage.inMemory());
   const runner=new runnerModule.ExtensionRunner([extension],runtime,'/fixture',manager,registry);
   runner.bindCore({} as any,{getModel:()=>undefined,getSystemPrompt:()=>'',getSignal:()=>undefined} as any);
   let trace:any[]=[];
   runner.setUIContext({custom:async(factory:any)=>{
    trace.push(['custom']);let result:any;
    const write=process.stdout.write;
    process.stdout.write=((s:any)=>{if(s!=='\x1b[2J\x1b[H')throw new Error('unexpected screen write');return true;}) as any;
    try{
     const component=factory({stop:()=>trace.push(['stop']),start:()=>trace.push(['start']),requestRender:(force=false)=>trace.push(['render',force])},{},{},(v:any)=>{result=v;trace.push(['done',v]);});
     if(component.render(80).length)throw new Error('empty component rendered');component.invalidate();component.dispose?.();return result;
    }finally{process.stdout.write=write;}
   }} as any,request.mode);
   const results=[];
   for(const command of request.commands){trace=[];const value=await runner.emitUserBash({type:'user_bash',command,excludeFromContext:false,cwd:'/fixture'});results.push({result:value?.result??null,trace});}
   return results;
  }finally{keys.forEach((k,i)=>{if(previous[i]===undefined)delete process.env[k];else process.env[k]=previous[i];});}
 }
 process.stdin.setEncoding('utf8');let pending='';for await(const chunk of process.stdin){pending+=chunk;let end;while((end=pending.indexOf('\n'))!==-1){const request=JSON.parse(pending.slice(0,end));pending=pending.slice(end+1);console.log(JSON.stringify(await run(request)));}}
 if(pending)throw new Error('unterminated request');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
