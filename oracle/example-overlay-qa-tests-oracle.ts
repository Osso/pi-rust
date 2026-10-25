import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {dirname, resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import vm from 'node:vm';
import readline from 'node:readline';
const source=process.argv[2];
const base=resolve(dirname(source),'../../..');
const moduleUrl=(path:string)=>pathToFileURL(resolve(base,path)).href;
async function main(){
const [loader,runnerModule,session,auth,utils,events,tui]=await Promise.all([
 import(moduleUrl('coding-agent/src/core/extensions/loader.ts')),
 import(moduleUrl('coding-agent/src/core/extensions/runner.ts')),
 import(moduleUrl('coding-agent/src/core/session-manager.ts')),
 import(moduleUrl('coding-agent/src/core/auth-storage.ts')),
 import(moduleUrl('coding-agent/src/core/model-registry.ts')),
 import(moduleUrl('coding-agent/src/core/event-bus.ts')),
 import(moduleUrl('tui/src/index.ts')),
]);
const ts=createRequire(resolve(base,'../package.json'))('typescript');
async function run(request:any) {
 let now=0,nextId=0;
 const timers=new Map<number,{callback:()=>void,at:number,period:number}>();
 const schedule=(callback:()=>void,ms:number,period=0)=>{const id=++nextId;timers.set(id,{callback,at:now+Math.floor(ms),period});return id;};
 const advance=(ms:number)=>{const end=now+ms;while(true){const entry=[...timers].filter(([,t])=>t.at<=end).sort((a,b)=>a[1].at-b[1].at)[0];if(!entry)break;now=entry[1].at;if(entry[1].period)entry[1].at+=entry[1].period;else timers.delete(entry[0]);entry[1].callback();}now=end;};
 const procHooks:any={};
 let pendingTail:string|undefined;
 const realDate=Date;
 const context=vm.createContext({exports:{},require:(name:string)=>{
  if(name==='@earendil-works/pi-tui')return tui;
  if(name==='child_process')return {spawn:()=>({stdout:{on(_event:string,callback:any){procHooks.stdout=callback;}},stderr:{on(_event:string,callback:any){procHooks.stderr=callback;}},on(_event:string,callback:any){procHooks.close=callback;},kill(){}})};
  throw new Error(`Unexpected import ${name}`);
 },setInterval:(fn:any,ms:number)=>schedule(fn,ms,Math.floor(ms)),clearInterval:(id:number)=>timers.delete(id),setTimeout:(fn:any,ms:number)=>schedule(fn,ms),clearTimeout:(id:number)=>timers.delete(id),Date:class extends realDate {constructor(){super(now);}static now(){return now;}}});
 const javascript=ts.transpileModule(readFileSync(source,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 vm.runInContext(javascript,context);
 const runtime=loader.createExtensionRuntime();
 const extension=await loader.loadExtensionFromFactory(context.exports.default,'/fixture',events.createEventBus(),runtime,source);
 const registry=utils.ModelRegistry.inMemory(auth.AuthStorage.inMemory());
 const runner=new runnerModule.ExtensionRunner([extension],runtime,'/fixture',session.SessionManager.inMemory('/fixture'),registry);
 const calls:any[]=[];
 runner.bindCore({sendMessage(){},sendUserMessage(){},appendEntry(){},requestResumeContinuation(){},setSessionName(){},getSessionName(){},setLabel(){},getActiveTools:()=>[],getAllTools:()=>[],setActiveTools(){},callTool:async()=>({content:[]}),callCommand:async()=>{},refreshTools(){},getCommands:()=>[],setModel:async()=>false,getThinkingLevel:()=> 'off',setThinkingLevel(){}},{getModel(){},isIdle:()=>true,hasActiveRetry:()=>false,isProjectTrusted:()=>true,getSignal(){},abort(){},hasPendingMessages:()=>false,shutdown(){},restart:async()=>{},getContextUsage(){},compact(){},getSystemPrompt:()=>'',getScopedModels:()=>[]});
 const roles=['accent','dim','muted','text','success','error','warning','toolTitle','borderMuted','border','selectedBg'];
 const theme={fg:(role:string,text:string)=>`\x1b[38;2;${50+roles.indexOf(role)};32;32m${text}\x1b[39m`};
 let invocation=0;
 const ui={setEditorText:(text:string)=>calls.push(['editorText',text]),notify:(message:string,level:string)=>calls.push(['notify',message,level]),custom:async(factory:any,options:any)=>{
  const script=request.scripts?.[invocation++]??{};
  const panels:any[]=[];let focus=-1,requests=0;
  const handle=(index:number)=>({hide(){panels[index].hidden=true;if(focus===index){focus=-1;panels[index].component.focused=false;}},setHidden(value:boolean){panels[index].hidden=value;},isHidden:()=>panels[index].hidden,focus(){focus=index;panels.forEach((p,i)=>p.component.focused=i===index);},unfocus(){if(focus===index)focus=-1;panels[index].component.focused=false;},isFocused:()=>focus===index});
  const host={requestRender(){requests++;},showOverlay(component:any,options:any){const index=panels.length;panels.push({component,options,hidden:false});return handle(index);}};
  let completed=false,value:any=null,doneCount=0;
  const component=factory(host,theme,{},(v:any)=>{doneCount++;completed=true;value=v??null;});
  component.focused=true;
  const mainHandle={hide(){},setHidden(value:boolean){calls.push(['hidden',value]);},isHidden:()=>false,focus(){},unfocus(){},isFocused:()=>true};
  options.onHandle?.(mainHandle);
  const cloned=script.clone?factory(host,theme,{},()=>{}):null;
  if(cloned)options.onHandle?.({...mainHandle,setHidden(value:boolean){calls.push(['hiddenClone',value]);}});
  const serial=(o:any)=>({...o,margin:typeof o.margin==='number'?{top:o.margin,right:o.margin,bottom:o.margin,left:o.margin}:o.margin,visible:o.visible?[o.visible(99,40),o.visible(100,40)]:undefined});
  calls.push(['options',serial(options.overlayOptions)]);
  const encodeLines = (lines: string[]) => script.rawUnitArrays
   ? lines.map(line => Array.from({ length: line.length }, (_, index) => line.charCodeAt(index)))
   : lines;
  const snapshot=(width:number)=>calls.push(['frames',encodeLines(component.render(width)),panels.map(p=>({options:serial(p.options),hidden:p.hidden,lines:encodeLines(p.component.render(p.options.width??34))}))]);
  snapshot(script.width??60);
  for (const step of script.steps ?? []) {
   if (step.proc?.exitBeforeTail !== undefined) pendingTail = step.proc.exitBeforeTail;
   if (step.releaseTail) {
    if (pendingTail === undefined) throw new Error('no pending final process output');
    procHooks.stdout(Buffer.from(pendingTail));
    procHooks.close();
   }
   if (step.running) {
    const finished = component.render(script.width ?? 60).join('\n').includes('✓ Done');
    if (finished) throw new Error('exit must not finish before stdio closes');
   }
   if (step.proc) {
    if (step.proc.stdout !== undefined) procHooks.stdout(Buffer.from(step.proc.stdout));
    if (step.proc.stderr !== undefined) procHooks.stderr(Buffer.from(step.proc.stderr));
    if (step.proc.close) procHooks.close();
   }
   if (step.dispose) component.dispose?.();
   if (step.ms) advance(step.ms);
   if (step.key !== undefined || step.units) {
    const target = step.panel === undefined ? component : panels[step.panel].component;
    target.handleInput(step.key ?? String.fromCharCode(...step.units));
   }
   snapshot(step.width ?? script.width ?? 60);
  }
  component.invalidate();component.dispose?.();cloned?.dispose?.();
  calls.push(['requests',requests]);
  calls.push(['done',completed,value,doneCount]);
  if(script.delay)await new Promise<void>(resolve=>schedule(resolve,script.delay));
  return value;
 }};
 runner.setUIContext(ui,'tui');
 const command=runner.getCommand(request.command);
 const promise=command.handler('',runner.createCommandContext());
 // Stacked custom calls are started before each source sleep; advance only after they register.
 for(let i=0;i<100;i++){await Promise.resolve();advance(100);}
 await promise;
 return {commands:[...extension.commands].map(([name,c]:any)=>[name,c.description]),calls};
}
for await(const line of readline.createInterface({input:process.stdin})) {
 try {console.log(JSON.stringify(await run(JSON.parse(line))));}
 catch(error:any){console.error(error);process.exit(1);}
}

}
main().catch(error=>{console.error(error);process.exit(1);});
