import { createReadonlySessionManager } from "./extension-examples-l-readonly-session-manager.ts";
import { readFile, rm } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { resolve, dirname } from "node:path";

// Execute unchanged pinned examples through their real native TS loader/runner.
// No discovery, live credentials, settings, providers, or session databases.
async function main() {
const source = process.argv[2];
const packages = resolve(source.slice(0, source.indexOf("/packages/") + "/packages/".length));
const moduleUrl = (path: string) => pathToFileURL(resolve(packages, path)).href;
const [{ default: factory }, loader, runnerModule, session, auth, utils, events, themeModule, fauxModule, keysModule] = await Promise.all([
 import(pathToFileURL(source).href),
 import(moduleUrl("coding-agent/src/core/extensions/loader.ts")),
 import(moduleUrl("coding-agent/src/core/extensions/runner.ts")),
 import(moduleUrl("coding-agent/src/core/session-manager.ts")),
 import(moduleUrl("coding-agent/src/core/auth-storage.ts")),
 import(moduleUrl("coding-agent/src/core/model-registry.ts")),
 import(moduleUrl("coding-agent/src/core/event-bus.ts")),
 import(moduleUrl("coding-agent/src/modes/interactive/theme/theme.ts")),
 import(moduleUrl("ai/src/providers/faux.ts")),
 import(moduleUrl("tui/src/keybindings.ts")),
]);
async function run(request: any) {
 keysModule.setKeybindings(new keysModule.KeybindingsManager(keysModule.TUI_KEYBINDINGS,request.keybindings ?? {}));
 const cwd = request.cwd ?? "/fixture";
 if (request.home) { process.env.HOME = request.home; process.chdir(cwd); }
 const runtime = loader.createExtensionRuntime();
 const extension = await loader.loadExtensionFromFactory(factory, cwd, events.createEventBus(), runtime, source);
 const registry = utils.ModelRegistry.inMemory(auth.AuthStorage.inMemory());
 const modelCalls:any[]=[];
 let model:any;
 if(request.modelFixture) {
  const fixture=request.modelFixture;
  const faux=fauxModule.fauxProvider({provider:fixture.provider ?? "google",models:fixture.missing ? [] : [{id:fixture.id ?? "gemini-2.5-flash"}],tokensPerSecond:1_000_000});
  model=faux.getModel();
  faux.setResponses(Array.from({length:20},()=> async (context:any,options:any,_state:any,model:any)=>{
   if(options.sessionId && !/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(options.sessionId)) throw new Error("invalid UUID v7");
   modelCalls.push({model:[model.provider,model.id],context,options:{maxTokens:options.maxTokens ?? null,cacheRetention:options.cacheRetention ?? null,sessionId:options.sessionId ? "<uuid-v7>" : null,aborted:options.signal?.aborted ?? false}});
   if(fixture.error) throw new Error(fixture.error);
   return fauxModule.fauxAssistantMessage(fixture.content ?? [{type:"text",text:"fixture answer"}],{stopReason:fixture.stopReason ?? "stop",timestamp:0});
  }));
  utils.getModelRuntime(registry).registerNativeProvider(faux.provider);
 }
 const manager = session.SessionManager.inMemory(cwd);
 const runner = new runnerModule.ExtensionRunner([extension], runtime, cwd, manager, registry);
 if (request.fixed) {
  const original = runner.createContext.bind(runner);
  runner.createContext = () => ({...original(),sessionManager:createReadonlySessionManager(manager)});
 }
 const effects:any[]=[];
 runner.bindCore({
  sendMessage() {}, sendUserMessage() {}, appendEntry(kind:string,data:any) {manager.appendCustomEntry(kind,data);effects.push(["appendEntry",kind,data]);}, requestResumeContinuation() {},
  setSessionName() {}, getSessionName() {}, setLabel() {}, getActiveTools: () => request.activeTools ?? [], getAllTools: () => (request.allTools ?? []).map((name:string)=>({name})),
  setActiveTools(names:string[]) {effects.push(["setActiveTools",names]);}, callTool: async () => ({content: []}), callCommand: async () => undefined,
  refreshTools() {}, getCommands: () => [], setModel: async () => false,
  getThinkingLevel: () => "off", setThinkingLevel() {},
 }, {
  getModel() {return request.hasModel === false ? undefined : model;}, isIdle: () => true, hasActiveRetry: () => false, isProjectTrusted: () => true,
  getSignal() {}, abort() {}, hasPendingMessages: () => false, shutdown() {}, restart: async () => {},
  getContextUsage() {}, compact() {}, getSystemPrompt: () => "", getScopedModels: () => [],
 });
 const errors: unknown[] = [];
 runner.onError((error: any) => errors.push([error.event, error.error]));
 const metadata = {commands: [...extension.commands.keys()], tools: [...extension.tools.keys()]};
 const cases = [];
 const originalDate = Date;
 if (request.nowMillis !== undefined) {
  globalThis.Date = class extends originalDate {
   static now() {return request.nowMillis;}
   constructor(...args: any[]) { super(...(args.length ? args : [request.nowMillis]) as [any]); }
  } as DateConstructor;
 }
 for (const item of request.cases) {
  const calls: any[] = [];
  const roles=["accent","dim","muted","text","success","error","warning","toolTitle","borderMuted","border","selectedBg"];
  const theme={fg:(role:string,text:string)=>`\x1b[38;2;${50+roles.indexOf(role)};32;32m${text}\x1b[39m`,bold:(text:string)=>text};
  themeModule.setThemeInstance(theme as any);
  const ui = {
   theme,
   custom: async (factory:any) => {
    let completed=false, value:any;
    const component=factory({requestRender(){},requestComponentRender(){}},theme,{},(v:any)=>{if(!completed){completed=true;value=v;}});
    const width=item.width ?? 40;
    calls.push(["render",component.render(width)]);
    for (const key of item.keys ?? []) {component.handleInput?.(key);calls.push(["render",component.render(width)]);}
    component.invalidate();calls.push(["render",component.render(width)]);
    for(let i=0;i<1000 && !completed;i++) await new Promise(r=>setTimeout(r,1));
    component.dispose?.();
    if(!completed) throw new Error("custom fixture never completed");
    return value;
   },
   setEditorText: (text:string)=>calls.push(["editorText",text]),
   notify: (message: string, level: string) => calls.push(["notify", message, level]),
   select: async (title: string, choices: string[]) => { calls.push(["select", title, choices]); return item.selected ?? undefined; },
   setWidget: (key: string, content: string[], options?: unknown) => calls.push(["widget", key, content, options ?? null]),
   setWorkingMessage: (message: string) => calls.push(["workingMessage", message]),
   setWorkingIndicator: (options: unknown) => calls.push(["workingIndicator", options]),
  };
  runner.setUIContext(item.hasUI === false ? undefined : ui, item.mode ?? "tui");
  let result: unknown;
  switch (item.event) {
   case "session_start": result = await runner.emit({type: "session_start", reason: "startup"}); break;
   case "append_custom": manager.appendCustomEntry(item.kind,item.data);break;
   case "append": manager.appendMessage(item.message); break;
   case "branch": manager.branch(manager.getEntries()[item.index].id); break;
   case "entries": result = manager.getEntries().map(({id,parentId,timestamp,...entry}:any)=>entry); break;
   case "event": {
    const controller=new AbortController();if(item.aborted) controller.abort();
    result=await runner.emit({...item.value,signal:controller.signal});break;
   }
   case "render_call": result=runner.getToolDefinition(item.toolName).renderCall(item.params,theme,{}).render(item.width ?? 40); break;
   case "render_result": result=runner.getToolDefinition(item.toolName).renderResult(item.result,{expanded:item.expanded ?? false,isPartial:item.isPartial ?? false},theme,{}).render(item.width ?? 40); break;
   case "command":
    try { result = await runner.getCommand(item.name).handler(item.args ?? "",runner.createCommandContext()); }
    catch (error: any) { result = {error:error.message}; }
    break;
   case "tool_call": result = await runner.emitToolCall({type:"tool_call", toolName:item.toolName,toolCallId:"fixture-call",bypassPermissions:false,input:item.input}); break;
   case "input": result = await runner.emitInput(item.text,item.images ?? undefined,item.source,item.streamingBehavior ?? undefined); break;
   case "tool_metadata": {
    const tool = runner.getToolDefinition(item.toolName ?? "bash");
    result = {name:tool.name,label:tool.label,description:tool.description,parameters:tool.parameters,promptSnippet:tool.promptSnippet ?? null,promptGuidelines:tool.promptGuidelines ?? null};
    break;
   }
   case "execute": {
    const tool = runner.getToolDefinition(item.toolName ?? "bash");
    try { result = {value: await tool.execute("fixture-call",item.params,undefined,undefined,runner.createContext())}; }
    catch (error: any) { result = {error:error.message}; }
    break;
   }
   default: throw new Error(`Unknown fixture event ${item.event}`);
  }
  if(request.captureOutputFiles && result?.value?.details?.fullOutputPath) {
   const path=result.value.details.fullOutputPath;
   if(!/\/pi-rg-[^/]+\/output\.txt$/.test(path)) throw new Error("unexpected output path");
   result.outputFile=await readFile(path,"utf8");
   result.value.details.fullOutputPath="<temporary>/pi-rg/output.txt";
   result.value.content[0].text=result.value.content[0].text.replaceAll(path,"<temporary>/pi-rg/output.txt");
   await rm(dirname(path),{recursive:true});
  }
  cases.push(JSON.parse(JSON.stringify({result: result ?? null, ui: calls})));
 }
 globalThis.Date = originalDate;
 return {metadata,cases,errors,...(request.allTools ? {effects}: {}),...(request.modelFixture ? {modelCalls}: {})};
}
process.stdin.setEncoding("utf8");
let pending = "";
for await (const chunk of process.stdin) {
 pending += chunk;
 let end: number;
 while ((end = pending.indexOf("\n")) !== -1) {
  const line = pending.slice(0, end); pending = pending.slice(end + 1);
  console.log(JSON.stringify(await run(JSON.parse(line))));
 }
}
if (pending.length) throw new Error("Unterminated oracle request");
}
main().catch(error => { console.error(error); process.exitCode = 1; });
