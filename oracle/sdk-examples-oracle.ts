import { readFileSync } from "node:fs";
import vm from "node:vm";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { format } from "node:util";
const ts = createRequire(resolve(dirname(process.argv[2]), "../../../..", "package.json"))("typescript");

// Execute the complete pinned program with SDK/config/provider I/O replaced at
// the import boundary. Console output uses Node's real formatter.
const source = readFileSync(process.argv[2], "utf8");
async function run(request: any) {
 const logs: any[] = [], prompts: any[] = [], disposed: number[] = [], created: any[] = [];
 let stdout = "", stderr = "";
 const append = (...args: any[]) => { logs.push(args); stdout += format(...args) + "\n"; };
 const core = resolve(dirname(process.argv[2]), "../../src/core");
 const [{ AuthStorage }, { ModelRegistry }, compat] = await Promise.all([
  import(pathToFileURL(resolve(core,"auth-storage.ts")).href),
  import(pathToFileURL(resolve(core,"model-registry.ts")).href),
  import(pathToFileURL(resolve(core,"../../../ai/src/compat.ts")).href),
 ]);
 const createFixtureRegistry = (storage: any) => {
  const registry = ModelRegistry.inMemory(storage);
  const groups = new Map<string, Map<string, any>>();
  for (const model of [...(request.models ?? []), ...(request.available ?? [])]) {
   let group = groups.get(model.provider);
   if (!group) { group = new Map(); groups.set(model.provider,group); }
   group.set(model.id,model);
  }
  for (const [provider,byId] of groups) {
   const models = [...byId.values()];
   registry.registerProvider(provider,{
    api:models[0].api,baseUrl:models[0].baseUrl,models,
    ...(request.available?.some((model: any)=>model.provider===provider) ? {apiKey:"fixture"} : {}),
   });
  }
  return registry;
 };
 class Loader {
  skills: any; prompts: any; agents: any;
  customPrompt?: string;
  appendPrompts?: string[];
  handlers: (() => void)[] = [];
  constructor(public options: any) {
   for (const factory of options.extensionFactories ?? []) factory({on: (event: string, handler: () => void) => {
    if (event === "agent_start") this.handlers.push(handler);
   }});
  }
  async reload() {
   this.customPrompt = this.options.systemPromptOverride?.(undefined);
   this.appendPrompts = this.options.appendSystemPromptOverride?.([]);
   this.skills = this.options.skillsOverride?.(request.skills ?? {skills:[], diagnostics:[]}) ?? {skills:[], diagnostics:[]};
   this.prompts = this.options.promptsOverride?.(request.prompts ?? {prompts:[], diagnostics:[]}) ?? {prompts:[], diagnostics:[]};
   this.agents = this.options.agentsFilesOverride?.(request.agents ?? {agentsFiles:[]}) ?? {agentsFiles:[]};
  }
  getSystemPrompt() { return this.customPrompt; }
  getAppendSystemPrompt() { return this.appendPrompts ?? []; }
  getSkills() { return this.skills; }
  getPrompts() { return this.prompts; }
  getAgentsFiles() { return this.agents; }
 }
 const sdk = {
  AuthStorage: { create: () => AuthStorage.inMemory(request.credentials ?? {}), inMemory: (data: any) => AuthStorage.inMemory(data) },
  ModelRegistry: { create: createFixtureRegistry, inMemory: (storage: any) => ModelRegistry.inMemory(storage) },
  DefaultResourceLoader: Loader, getAgentDir: () => "/fixture/agent",
  createSyntheticSourceInfo: (path: string, options: any) => ({path, ...options}),
  SessionManager: {
   inMemory: () => ({}),
   create: () => ({file: request.runtimeFiles?.[0] ?? request.newFile}),
   continueRecent: () => ({file: request.continuedFile}),
   list: async () => request.sessions ?? [],
   open: (path: string) => ({file:path, id:request.sessions?.find((s: any) => s.path === path)?.id}),
  },
  SettingsManager: class {
   static create() { return new this(); }
   static inMemory() { return new this(); }
   getGlobalSettings() { return request.settings ?? {}; }
   applyOverrides() {}
   setDefaultThinkingLevel() {}
   async flush() {}
   drainErrors() { return request.settingsErrors ?? []; }
  },
  async createAgentSessionServices() { return {diagnostics:[]}; },
  async createAgentSessionFromServices(options: any) { return sdk.createAgentSession(options); },
  async createAgentSessionRuntime(factory: any, options: any) {
   const initial = await factory(options);
   const runtime = {
    session: initial.session,
    async newSession() { await this.session.abort(); this.session.dispose(); this.session = (await factory({...options,sessionManager:{file:request.runtimeFiles?.[1]}})).session; },
    async switchSession(path: string) { await this.session.abort(); this.session.dispose(); this.session = (await factory({...options,sessionManager:{file:path}})).session; },
    async dispose() { this.session.dispose(); },
   };
   return runtime;
  },
  async createAgentSession(options: any = {}) {
   const id = created.length;
   created.push({tools:options.tools, thinkingLevel:options.thinkingLevel, model:options.model, customPrompt:options.resourceLoader?.getSystemPrompt?.(), appendPrompts:options.resourceLoader?.getAppendSystemPrompt?.()});
   let observer: any;
   return { session: {
    sessionFile: options.sessionManager?.file,
    sessionId: options.sessionManager?.id,
    // Pinned runtime teardownCurrent:179–186 calls abort; pinned
    // AgentSession.abort:3749–3756 emits an empty queue update before disposal.
    async abort() { observer?.({type:"queue_update",steering:[],followUp:[]}); },
    async bindExtensions() {},
    subscribe(listener: any) { observer = listener; return () => {observer = undefined;}; },
    async prompt(prompt: string) {
     prompts.push(prompt);
     for (const handler of options.resourceLoader?.handlers ?? []) handler();
     observer?.({type:"message_update",assistantMessageEvent:{type:"text_delta",delta:request.text ?? "Hello!"}});
     stdout += "";
     if (request.failure) throw new Error(request.failure);
    },
    dispose() { disposed.push(id); }
   }};
  }
 };
 const context = vm.createContext({
  process: { cwd: () => "/fixture/project", env: {}, stdout: {write(text: string) {if (request.writeFailure) throw new Error(request.writeFailure); stdout += text;}} },
  console: {log:append, warn: (...args: any[]) => {logs.push(args); stderr += format(...args) + "\n";} },
  require: (name: string) => {
   if (name === "@earendil-works/pi-coding-agent") return {...sdk, createExtensionRuntime: () => ({})};
   if (name === "@earendil-works/pi-ai/compat") return compat;
   throw new Error(`Unexpected import ${name}`);
  },
  exports: {},
 });
 const javascript = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 let error = null, errorName = null;
 try { await vm.runInContext(`(async () => {${javascript}\n})()`,context); }
 catch (failure: any) { error = failure.message; errorName = failure.name; }
 return {stdout, stderr, logs, prompts, disposed, created, error, errorName};
}
async function main() {
let pending = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) {
 pending += chunk;
 let end: number;
 while ((end = pending.indexOf("\n")) !== -1) {
  const line = pending.slice(0,end); pending = pending.slice(end+1);
  console.log(JSON.stringify(await run(JSON.parse(line))));
 }
}
if (pending.length) throw new Error("Unterminated oracle request");
}
main().catch(error => { console.error(error); process.exit(1); });
