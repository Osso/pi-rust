// Model/session UI seams extending the A/B real loader/runner approach.
import {pathToFileURL} from 'node:url';
import {join} from 'node:path';
async function main(){
 const source=process.argv[2],packages=source.slice(0,source.indexOf('/packages/')+10);
 const load=(name:string)=>import(pathToFileURL(join(packages,'coding-agent/src',name)).href);
 const [{default:factory},loader,runners,sessions,events,auth,utils,fauxModule,themeModule,compat]=await Promise.all([import(pathToFileURL(source).href),load('core/extensions/loader.ts'),load('core/extensions/runner.ts'),load('core/session-manager.ts'),load('core/event-bus.ts'),load('core/auth-storage.ts'),import(pathToFileURL(join(packages,'coding-agent/src/core/model-registry.ts')).href),import(pathToFileURL(join(packages,'ai/src/providers/faux.ts')).href),load('modes/interactive/theme/theme.ts'),import(pathToFileURL(join(packages,'ai/src/compat.ts')).href)]);
 async function run(request:any){
  const runtime=loader.createExtensionRuntime(),extension=await loader.loadExtensionFromFactory(factory,'/fixture',events.createEventBus(),runtime,'<example:session>');
  const metadata={commands:[...extension.commands.values()].map((c:any)=>[c.name,c.description,!!c.getArgumentCompletions]),tools:[...extension.tools.keys()]};
  const faux=compat.registerFauxProvider({api:compat.getModel('openai','gpt-5.2').api,provider:'openai',models:[{id:request.missingModel?'other':'gpt-5.2',name:'Fixture',reasoning:true,contextWindow:128000,maxTokens:4096}]});
  const credentials=auth.AuthStorage.inMemory(request.noAuth?{}:{openai:{type:'api_key',key:'fixture-key'}});
  const registry=utils.ModelRegistry.inMemory(credentials);
  const calls:any[]=[],trace:any[]=[],errors:any[]=[];
  faux.setResponses([async(context:any,options:any,_state:any,model:any)=>{
   if(options.sessionId!==undefined && !/^[0-9a-f]{8}-[0-9a-f]{4}-7/.test(options.sessionId))throw new Error('invalid uuidv7');
   const captured=JSON.parse(JSON.stringify(context));captured.messages[0].timestamp='<clock>';
   calls.push({model:[model.provider,model.id],context:captured,options:{cacheRetention:options.cacheRetention??null,sessionId:options.sessionId===undefined?null:'<uuidv7>',reasoningEffort:options.reasoningEffort??null}});
   if(request.error)throw new Error(request.error);
   return fauxModule.fauxAssistantMessage(request.response??[{type:'text',text:'## Summary\n\nDone **one**.'},{type:'thinking',thinking:'hidden'},{type:'text',text:'Next: `two`.'}],{timestamp:0,stopReason:request.aborted?'aborted':'stop'});
  }]);
  const manager=sessions.SessionManager.inMemory('/fixture');if(request.sessionFile)manager.setSessionFile(request.sessionFile);for(const message of request.messages??[])manager.appendMessage(message);
  if(request.compaction!==undefined){const id=manager.getEntries()[request.compaction]?.id??'missing';manager.appendCompaction('Earlier summary',id,12000);manager.appendMessage({role:'user',content:'after compaction',timestamp:0});}
  const runner=new runners.ExtensionRunner([extension],runtime,'/fixture',manager,registry);
  runner.bindCore({sendMessage(){},sendUserMessage(){},appendEntry(){},requestResumeContinuation(){},setSessionName(){},getSessionName(){},setLabel(){},getActiveTools:()=>[],getAllTools:()=>[],setActiveTools(){},callTool:async()=>({content:[]}),callCommand:async()=>undefined,refreshTools(){},getCommands:()=>[],setModel:async()=>false,getThinkingLevel:()=>"off",setThinkingLevel(){}},{getModel:()=>request.noModel?undefined:registry.find('openai','gpt-5.2'),isIdle:()=>true,hasActiveRetry:()=>false,isProjectTrusted:()=>true,getSignal(){},abort(){},hasPendingMessages:()=>false,shutdown(){},restart:async()=>{},getContextUsage(){},compact(){},getSystemPrompt:()=>'',getScopedModels:()=>[]});
  themeModule.initTheme('dark',false);
  const ui={theme:themeModule.theme,notify:(s:string,level:string)=>trace.push(['notify',s,level]),editor:async(title:string,prefill:string)=>{trace.push(['editor',title,prefill]);return request.editor;},setEditorText:(s:string)=>trace.push(['setEditorText',s]),custom:async(factory:any)=>{
   let completed=false,resolve:any;const result=new Promise(done=>{resolve=done;});
   const component=factory({requestRender(){},requestComponentRender(){},terminal:{rows:40,columns:80}},themeModule.theme,{},(value:any)=>{completed=true;resolve(value);});
   for(const width of [32,80])trace.push(['render',width,component.render(width)]);
   component.invalidate();component.handleInput?.('x');component.handleInput?.(request.key??'\r');
   const timer=setTimeout(()=>resolve(Promise.reject(new Error('custom UI did not complete'))),3000);
   try{return await result;}finally{clearTimeout(timer);component.dispose?.();}
  }};
  const transitions:any[]=[],replacementUi:any[]=[];
  const replacement=new runners.ExtensionRunner([],loader.createExtensionRuntime(),'/fixture',sessions.SessionManager.inMemory('/fixture'),registry);
  replacement.setUIContext({notify:(s:string,level:string)=>replacementUi.push(['notify',s,level]),setEditorText:(s:string)=>replacementUi.push(['setEditorText',s])},'tui');
  runner.bindCommandContext({newSession:async(options:any)=>{transitions.push(['newSession',options.parentSession??null,!!options.setup,!!options.withSession]);if(!request.cancelSession){runner.invalidate();await options.withSession?.(replacement.createCommandContext());}return {cancelled:!!request.cancelSession};}});
  runner.setUIContext(request.hasUI===false?undefined:ui,request.mode??'tui');runner.onError((e:any)=>errors.push([e.event,e.error]));
  let result:any=null;try{await runner.getCommand(source.includes('handoff.ts')?'handoff':'summarize').handler(request.args??'',runner.createCommandContext());}catch(e:any){result=e.message;}
  faux.unregister();
  return {metadata,trace,calls,errors,result,transitions,replacementUi};
 }
 let pending='';process.stdin.setEncoding('utf8');for await(const chunk of process.stdin){pending+=chunk;let end;while((end=pending.indexOf('\n'))!==-1){const line=pending.slice(0,end);pending=pending.slice(end+1);console.log(JSON.stringify(await run(JSON.parse(line))));}}
 if(pending)throw new Error('Unterminated oracle request');
}
main().catch(e=>{console.error(e);process.exitCode=1;});
