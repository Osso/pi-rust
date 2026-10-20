import { pathToFileURL } from "node:url";
import { resolve, dirname } from "node:path";
import { existsSync, readFileSync } from "node:fs";

// Execute unchanged pinned examples through their real native TS loader/runner.
// No discovery, live credentials, settings, providers, or session databases.
async function main() {
const source = process.argv[2];
const packages = resolve(dirname(source), "../../..");
const moduleUrl = (path: string) => pathToFileURL(resolve(packages, path)).href;
const [{ default: factory }, loader, runnerModule, session, auth, utils, events] = await Promise.all([
 import(pathToFileURL(source).href),
 import(moduleUrl("coding-agent/src/core/extensions/loader.ts")),
 import(moduleUrl("coding-agent/src/core/extensions/runner.ts")),
 import(moduleUrl("coding-agent/src/core/session-manager.ts")),
 import(moduleUrl("coding-agent/src/core/auth-storage.ts")),
 import(moduleUrl("coding-agent/test/model-runtime-test-utils.ts")),
 import(moduleUrl("coding-agent/src/core/event-bus.ts")),
]);
async function run(request: any) {
 const cwd = request.cwd ?? "/fixture";
 if (request.home) { process.env.HOME = request.home; process.chdir(cwd); }
 const runtime = loader.createExtensionRuntime();
 const extension = await loader.loadExtensionFromFactory(factory, cwd, events.createEventBus(), runtime, source);
 const registry = await utils.createInMemoryModelRegistry(auth.AuthStorage.inMemory());
 const runner = new runnerModule.ExtensionRunner([extension], runtime, cwd, session.SessionManager.inMemory(cwd), registry);
 runner.bindCore({
  sendMessage() {}, sendUserMessage() {}, appendEntry() {}, requestResumeContinuation() {},
  setSessionName() {}, getSessionName() {}, setLabel() {}, getActiveTools: () => [], getAllTools: () => [],
  setActiveTools() {}, callTool: async () => ({content: []}), callCommand: async () => undefined,
  refreshTools() {}, getCommands: () => [], setModel: async () => false,
  getThinkingLevel: () => "off", setThinkingLevel() {},
 }, {
  getModel() {}, isIdle: () => true, hasActiveRetry: () => false, isProjectTrusted: () => true,
  getSignal() {}, abort() {}, hasPendingMessages: () => false, shutdown() {}, restart: async () => {},
  getContextUsage() {}, compact() {}, getSystemPrompt: () => "", getScopedModels: () => [],
 });
 const errors: unknown[] = [];
 runner.onError((error: any) => errors.push([error.event, error.error]));
 const metadata = {commands: [...extension.commands.keys()], tools: [...extension.tools.keys()]};
 const github = source.endsWith("/github-issue-autocomplete.ts");
 const allCalls: any[] = [];
 let wrapper: any;
 let provider: any = {
  getSuggestions: async () => ({items:[{value:"base",label:"Base",description:"delegate"}],prefix:"base-prefix"}),
  applyCompletion(lines: string[], line: number, col: number, item: any, prefix: string) { const updated=[...lines]; updated[line]=lines[line].slice(0,col-prefix.length)+item.value; return {lines:updated,cursorLine:line,cursorCol:col-prefix.length+item.value.length}; },
  shouldTriggerFileCompletion: () => false,
 };
 const cases = [];
 const originalDate = Date;
 if (request.nowMillis !== undefined) {
  globalThis.Date = class extends originalDate {
   constructor(...args: any[]) { super(...(args.length ? args : [request.nowMillis]) as [any]); }
  } as DateConstructor;
 }
 for (const item of request.cases) {
  const calls: any[] = github ? allCalls : [];
  const ui = {
   addAutocompleteProvider: (factory: any) => { wrapper=factory; },
   notify: (message: string, level: string) => calls.push(["notify", message, level]),
   select: async (title: string, choices: string[]) => { calls.push(["select", title, choices]); return item.selected ?? undefined; },
   setWidget: (key: string, content: string[], options?: unknown) => calls.push(["widget", key, content, options ?? null]),
   setWorkingMessage: (message: string) => calls.push(["workingMessage", message]),
   setWorkingIndicator: (options: unknown) => calls.push(["workingIndicator", options]),
  };
  runner.setUIContext(item.hasUI === false ? undefined : ui, "tui");
  let result: unknown;
  switch (item.event) {
   case "session_start": result = await runner.emit({type: "session_start", reason: "startup"}); break;
   case "autocomplete": {
    if (wrapper) { provider=wrapper(provider); wrapper=undefined; }
    const text=item.text, lines=[text], abort=new AbortController();
    if (item.aborted) abort.abort();
    const suggestions=await provider.getSuggestions(lines,0,text.length,{signal:abort.signal,force:false});
    result={suggestions,applied:provider.applyCompletion(lines,0,text.length,{value:"#101",label:"#101"},text),trigger:provider.shouldTriggerFileCompletion(lines,0,text.length)};
    break;
   }
   case "tool_call": result = await runner.emitToolCall({type:"tool_call", toolName:item.toolName,toolCallId:"fixture-call",bypassPermissions:false,input:item.input}); break;
   case "input": result = await runner.emitInput(item.text,item.images ?? undefined,item.source,item.streamingBehavior ?? undefined); break;
   case "tool_metadata": {
    const tool = runner.getToolDefinition("bash");
    result = {name:tool.name,label:tool.label,description:tool.description,parameters:tool.parameters,promptSnippet:tool.promptSnippet ?? null,promptGuidelines:tool.promptGuidelines ?? null};
    break;
   }
   case "execute": {
    const tool = runner.getToolDefinition("bash");
    try { result = {value: await tool.execute("fixture-call",item.params,undefined,undefined,runner.createContext())}; }
    catch (error: any) { result = {error:error.message}; }
    break;
   }
   default: throw new Error(`Unknown fixture event ${item.event}`);
  }
  cases.push({result: result ?? null, ui: github ? [] : calls});
 }
 globalThis.Date = originalDate;
 if (github) {
  const receipt=resolve(cwd,"requests");
  const requests=existsSync(receipt) ? readFileSync(receipt,"utf8").trim().split("\n").filter(Boolean).map(line=>JSON.parse(line)) : [];
  return {metadata,cases,errors,ui:allCalls,requests};
 }
 return {metadata,cases,errors};
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
