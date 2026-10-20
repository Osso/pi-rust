// Batch E extends A/B's loader/runner transcript pattern, with private files and git.
import { pathToFileURL } from "node:url";
import { resolve, dirname, join } from "node:path";
import * as fs from "node:fs";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
async function main() {
 const source=process.argv[2];
 const packages=resolve(source.slice(0,source.indexOf('/packages/')+10));
 const load=(name:string)=>import(pathToFileURL(join(packages,'coding-agent/src',name)).href);
 const [{default:factory}, loader, runners, sessions, events, auth, utils]=await Promise.all([
  import(pathToFileURL(source).href),load('core/extensions/loader.ts'),load('core/extensions/runner.ts'),load('core/session-manager.ts'),load('core/event-bus.ts'),load('core/auth-storage.ts'),import(pathToFileURL(join(packages,'coding-agent/test/model-runtime-test-utils.ts')).href)
 ]);
 async function run(request:any) {
  const cwd=fs.mkdtempSync(join(tmpdir(),'example-e-'));
  const git=(args:string[])=>execFileSync('git',args,{cwd,encoding:'utf8',env:{...process.env,GIT_CONFIG_GLOBAL:'/dev/null',GIT_CONFIG_SYSTEM:'/dev/null',GIT_CONFIG_NOSYSTEM:'1'},stdio:['ignore','pipe','pipe']});
  const write=(files:any)=>{for(const [name,content] of Object.entries(files??{})){ const file=join(cwd,name); if(content===null) {if(fs.existsSync(file))fs.unlinkSync(file);}else{fs.mkdirSync(dirname(file),{recursive:true});fs.writeFileSync(file,content as string);}}};
  try {
   if(request.git){git(['init','-q']);git(['config','user.name','Fixture']);git(['config','user.email','fixture@example.invalid']);write({'tracked.txt':'initial\n'});git(['add','-A']);git(['commit','-qm','initial']);if(request.rejectCommit)fs.symlinkSync('/usr/bin/false',join(cwd,'.git/hooks/pre-commit'));}
   write(request.files);
   const runtime=loader.createExtensionRuntime(),bus=events.createEventBus();
   const extension=await loader.loadExtensionFromFactory(factory,cwd,bus,runtime,source.split('/examples/extensions/')[1]);
   const manager=sessions.SessionManager.inMemory(cwd);
   const registry=await utils.createInMemoryModelRegistry(auth.AuthStorage.inMemory());
   const runner=new runners.ExtensionRunner([extension],runtime,cwd,manager,registry);
   let usage:any;
   const uiCalls:any[]=[],actions:any[]=[],errors:any[]=[];
   let selected:string|undefined,note:string|undefined;
   const ui={notify:(text:string,level:string)=>uiCalls.push(['notify',Buffer.from(text,'utf8').toString('utf8'),level]),select:async(title:string,items:string[])=>{uiCalls.push(['select',title,items]);return selected;},input:async(title:string,placeholder:string)=>{uiCalls.push(['input',title,placeholder]);return note;}};
   runner.bindCore({sendMessage(){},sendUserMessage(){},appendEntry(){},requestResumeContinuation(){},setSessionName(){},getSessionName(){},setLabel(){},getActiveTools:()=>[],getAllTools:()=>[],setActiveTools(){},callTool:async()=>({content:[]}),callCommand:async()=>undefined,refreshTools(){},getCommands:()=>[],setModel:async()=>false,getThinkingLevel:()=>"off",setThinkingLevel(){}},
    {getModel(){},isIdle:()=>true,hasActiveRetry:()=>false,isProjectTrusted:()=>true,getSignal(){},abort(){},hasPendingMessages:()=>false,shutdown(){},restart:async()=>{},getContextUsage:()=>usage,compact:(options:any)=>{actions.push(['compact',options.customInstructions??null]);if(request.compactOutcome==='error')options.onError?.(new Error('fixture compaction failure'));else options.onComplete?.({summary:'fixture summary',firstKeptEntryId:'kept',tokensBefore:120001});},getSystemPrompt:()=>'',getScopedModels:()=>[]});
   runner.onError((error:any)=>errors.push([error.event,error.error]));
   const cases=[];
   for(const step of request.steps){
    uiCalls.length=0;actions.length=0;selected=step.selected??undefined;note=step.note??undefined;
    const hasUI=step.hasUI!==false, mode=step.mode??'tui';runner.setUIContext(hasUI?ui:undefined,mode);
    let result:any;
    switch(step.op){
     case 'before':result=await runner.emitBeforeAgentStart('task',undefined,step.prompt??'base',step.options??{});break;
     case 'trust':{const emitted=await runners.emitProjectTrustEvent({extensions:[extension],errors:[]},{type:'project_trust',cwd},{cwd,mode,hasUI,ui:hasUI?ui:runner.createContext().ui}); result=emitted.result;for(const error of emitted.errors)errors.push([error.event,error.error]);break;}
     case 'resources':result=await runner.emitResourcesDiscover(cwd,step.reason);break;
     case 'command':try{await runner.getCommand(step.name).handler(step.args??'',runner.createCommandContext());}catch(error:any){result={error:error.message};}break;
     case 'usage':usage=typeof step.tokens==='number'?{tokens:step.tokens,contextWindow:200000,percent:step.tokens/2000}:undefined;break;
     case 'append':manager.appendMessage(step.message);break;
     case 'write':write(step.files);break;
     case 'git':result=git(step.args);break;
     case 'snapshot':result={files:Object.fromEntries(step.files.map((name:string)=>[name,fs.existsSync(join(cwd,name))?fs.readFileSync(join(cwd,name),'utf8'):null])),status:git(['status','--porcelain']),messages:git(['log','--format=%B%x00'])};break;
     case 'tool_result':result=await runner.emitToolResult(step.event);break;
     case 'event':{const event={...step.event};if(event.entryId==='$leaf')event.entryId=manager.getLeafId();else if(/^\$\d+$/.test(event.entryId??''))event.entryId=manager.getEntries()[Number(event.entryId.slice(1))].id;result=await runner.emit(event);break;}
     default:throw new Error(`Unknown op ${step.op}`);
    }
    cases.push({result:result??null,ui:[...uiCalls],actions:[...actions]});
   }
   const commands=runner.getRegisteredCommands().map((command:any)=>[command.name,command.description]);
   const output={commands,cases,errors};
   return JSON.parse(JSON.stringify(output).split(dirname(source)).join('<module>').split(cwd).join(source.endsWith('/dynamic-resources/index.ts')?'<cwd>':'<tmp>'));
  }finally{fs.rmSync(cwd,{recursive:true,force:true});}
 }
 let pending='';process.stdin.setEncoding('utf8');for await(const chunk of process.stdin){pending+=chunk;let end;while((end=pending.indexOf('\n'))!==-1){const line=pending.slice(0,end);pending=pending.slice(end+1);console.log(JSON.stringify(await run(JSON.parse(line))));}}
 if(pending)throw new Error('Unterminated oracle request');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
