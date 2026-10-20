import { readFileSync } from "node:fs";
import vm from "node:vm";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
const ts = createRequire(resolve(dirname(process.argv[2]), "../../../..", "package.json"))("typescript");

// Execute the complete pinned program with SDK/config/provider I/O replaced at
// the import boundary. Preserve console values; Rust prints arrays as JSON.
const source = readFileSync(process.argv[2], "utf8");
async function run(request: any) {
 const logs: any[] = [], prompts: any[] = [], disposed: number[] = [], created: any[] = [];
 let stdout = "";
 const append = (...args: any[]) => { logs.push(args); stdout += args.map(x => typeof x === "string" ? x : JSON.stringify(x)).join(" ") + "\n"; };
 class Runtime {
  static async create() { return new Runtime(); }
  getModel(provider: string, id: string) { return (request.models ?? []).find((m: any) => m.provider === provider && m.id === id); }
  async getAvailable() { return request.available ?? []; }
  async setRuntimeApiKey() {}
 }
 class Loader {
  skills: any; prompts: any; agents: any;
  handlers: (() => void)[] = [];
  constructor(public options: any) {
   for (const factory of options.extensionFactories ?? []) factory({on: (event: string, handler: () => void) => {
    if (event === "agent_start") this.handlers.push(handler);
   }});
  }
  async reload() {
   this.skills = this.options.skillsOverride?.(request.skills ?? {skills:[], diagnostics:[]}) ?? {skills:[], diagnostics:[]};
   this.prompts = this.options.promptsOverride?.(request.prompts ?? {prompts:[], diagnostics:[]}) ?? {prompts:[], diagnostics:[]};
   this.agents = this.options.agentsFilesOverride?.(request.agents ?? {agentsFiles:[]}) ?? {agentsFiles:[]};
  }
  getSkills() { return this.skills; }
  getPrompts() { return this.prompts; }
  getAgentsFiles() { return this.agents; }
 }
 const sdk = {
  ModelRuntime: Runtime, DefaultResourceLoader: Loader, getAgentDir: () => "/fixture/agent",
  createSyntheticSourceInfo: (path: string, options: any) => ({path, ...options}),
  SessionManager: {
   inMemory: () => ({}),
   create: () => ({file: request.newFile}),
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
  async createAgentSession(options: any = {}) {
   const id = created.length;
   created.push({tools:options.tools, thinkingLevel:options.thinkingLevel, model:options.model});
   let observer: any;
   return { session: {
    sessionFile: options.sessionManager?.file,
    sessionId: options.sessionManager?.id,
    subscribe(listener: any) { observer = listener; },
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
  process: { cwd: () => "/fixture/project", env: {}, stdout: {write(text: string) {stdout += text;}} },
  console: {log:append, warn:append},
  require: (name: string) => {
   if (name === "@earendil-works/pi-coding-agent") return {...sdk, createExtensionRuntime: () => ({})};
   if (name === "@earendil-works/pi-ai/compat") return {getModel: () => ({provider:"anthropic",id:"claude-sonnet-4-5"})};
   throw new Error(`Unexpected import ${name}`);
  },
  exports: {},
 });
 const javascript = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 let error = null, errorName = null;
 try { await vm.runInContext(`(async () => {${javascript}\n})()`,context); }
 catch (failure: any) { error = failure.message; errorName = failure.name; }
 return {stdout, logs, prompts, disposed, created, error, errorName};
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
