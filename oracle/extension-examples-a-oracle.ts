// Batch A: unchanged pinned examples, actual loader/runtime/runner/event bus and
// in-memory SessionManager. Only host actions/UI and clock are fixture seams.
import { dirname, resolve } from "node:path";
import fs from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { pathToFileURL } from "node:url";

async function main() {
const file = process.argv[2];
const root = resolve(dirname(file), "../../src");
const load = (path: string) => import(pathToFileURL(resolve(root, path)).href);
const [{default: register}, {createExtensionRuntime, loadExtensionFromFactory, loadExtensions}, {ExtensionRunner},
 {createEventBus}, {SessionManager}] = await Promise.all([
 import(pathToFileURL(file).href), load("core/extensions/loader.ts"), load("core/extensions/runner.ts"),
 load("core/event-bus.ts"), load("core/session-manager.ts"),
]);

async function run(request: any) {
 const trace: any[] = [];
 const cwd = request.cwd ?? "/fixture";
 const runtime = createExtensionRuntime(), bus = createEventBus();
 const watchers: fs.FSWatcher[] = [];
 const fileOps = {watch:fs.watch,readFileSync:fs.readFileSync,writeFileSync:fs.writeFileSync};
 let fileChanged: (() => void) | undefined;
 const physical = (path: any) => path === "/tmp/agent-trigger.txt" ? request.triggerPath : path;
 if (request.triggerPath) {
  fs.watch = ((path: any, callback: any) => {
   const watcher = fileOps.watch(physical(path), (...args: any[]) => { callback(...args); fileChanged?.(); });
   watchers.push(watcher); return watcher;
  }) as any;
  fs.readFileSync = ((path: any,...args: any[]) => (fileOps.readFileSync as any)(physical(path),...args)) as any;
  fs.writeFileSync = ((path: any,...args: any[]) => (fileOps.writeFileSync as any)(physical(path),...args)) as any;
  syncBuiltinESMExports();
 }
 const loaded = request.fileLoader ? await loadExtensions([file],cwd,bus,runtime) : undefined;
 if (loaded?.errors.length) throw new Error(JSON.stringify(loaded.errors));
 const extension = loaded?.extensions[0] ?? await loadExtensionFromFactory(register, cwd, bus, runtime, "<example:batch-a>");
 const manager = SessionManager.inMemory("/fixture");
 const observed: [string, unknown][] = [];
 const extensions = [extension];
 if (request.observeBus) {
  const observer = await loadExtensionFromFactory((pi) => {
   pi.events.on("my:notification", (data: unknown) => {
    observed.push(["my:notification", data]);
   });
  }, "/fixture", bus, runtime, "<observer:batch-a>");
  extensions.push(observer);
 }
 const runner = new ExtensionRunner(extensions, runtime, cwd, manager, {});
 let name: string | undefined = request.name, prompt = "", idle = true;
 let activeTools: string[] = [];
 let footer: any, branch = request.branch, branchCallback: (() => void) | undefined;
 const fg = (role: string, text: string) => {
  const rgb: any = {dim:"90;91;92",accent:"80;180;230",success:"34;204;102",toolTitle:"212;212;212",text:"238;238;238",muted:"128;128;128",error:"204;34;34",warning:"204;170;34"};
  return `\x1b[38;2;${rgb[role]}m${text}\x1b[39m`;
 };
 const theme = {fg, bg: (_role: string, text: string) => `\x1b[48;2;16;24;32m${text}\x1b[49m`, bold: (text: string) => text};
 const dialogs = [...(request.dialogs ?? [])];
 const timers = new Map<number, {callback: () => void, at: number, interval?: number}>();
 let clock = 0, timerId = 0;
 const advance = (ms: number) => {
  const end = clock + ms;
  while (true) {
   const next = [...timers.entries()].filter(([,t]) => t.at <= end).sort((a,b) => a[1].at-b[1].at)[0];
   if (!next) break;
   const [id,t] = next; clock = t.at;
   if (t.interval) t.at += t.interval; else timers.delete(id);
   t.callback();
  }
  clock = end;
 };
 runner.bindCore({
  sendMessage(message: any, options: any) {
   trace.push(["sendMessage",message,options]);
   if (request.sendError) throw new Error(request.sendError);
  },
  setSessionName(value: string) {
   if (request.actionError) throw new Error(request.actionError);
   name = value; trace.push(["setSessionName", value]);
  },
  getSessionName: () => name,
  appendEntry: (type: string, data: any) => { manager.appendCustomEntry(type,data); trace.push(["appendEntry",type,data]); },
  getActiveTools: () => [...activeTools],
  setActiveTools: (names: string[]) => { activeTools = [...names]; trace.push(["setActiveTools", names]); },
  sendUserMessage: (content: any, options?: any) => {
   if (request.sendError) throw new Error(request.sendError);
   trace.push(["sendUserMessage", content, options ?? null]);
  },
  setLabel(id: string, label?: string) {
   if (request.actionError) throw new Error(request.actionError);
   const index = manager.getEntries().findIndex((entry: any) => entry.id === id);
   manager.appendLabelChange(id, label);
   if (manager.getLabel(id) !== label) throw new Error("label mutation failed");
   trace.push(["setLabel", index, label ?? null]);
  },
 } as any, {getSystemPrompt: () => prompt, isIdle: () => idle,
  shutdown: () => trace.push(["shutdown"]), getModel: () => request.model} as any);
 runner.bindCommandContext({reload: async () => { trace.push(["reload"]); }} as any);
 runner.setUIContext({
  notify: (message: string, level: string) => trace.push(["notify", message, level]),
  setStatus: (key: string, text?: string) => trace.push(["status", key, text ?? null]),
  setWorkingIndicator: (options: any) => trace.push(["indicator",options ?? null]),
  setHiddenThinkingLabel: (label?: string) => trace.push(["thinkingLabel",label ?? null]),
  setTitle: (title: string) => trace.push(["title",title]),
  confirm: async (title: string, message: string, opts?: any) => {
   trace.push(["confirm",title,message,opts?.timeout ?? null,!!opts?.signal]);
   const answer = dialogs.shift();
   if (answer?.error) throw new Error(answer.error);
   if (answer === "abort") { advance(5000); return false; }
   return answer;
  },
  select: async (title: string, items: string[], opts?: any) => {
   trace.push(["select",title,items,opts?.timeout ?? null]);
   const answer = dialogs.shift();
   if (answer?.error) throw new Error(answer.error);
   return answer ?? undefined;
  },
  theme,
  setFooter: (factory?: any) => {
   trace.push(["setFooter",!!factory]);
   footer?.dispose?.();
   footer = factory?.({requestRender: () => trace.push(["requestRender"])}, theme, {
    getGitBranch: () => branch,
    onBranchChange: (callback: () => void) => { branchCallback = callback; return () => { branchCallback = undefined; }; },
   });
  },
 } as any, request.mode ?? "tui");
 if (request.noUi) runner.setUIContext(undefined, request.mode ?? "json");
 runner.onError((error: any) => trace.push(["handlerError",error.error]));
 const commands = [...extension.commands.values()].map((c: any) => [c.name, c.description, !!c.getArgumentCompletions]);
 const tools = [...extension.tools.values()].map((t: any) => {
  const d = t.definition; const row = [d.name, d.label, d.description, d.parameters];
  return request.metadata ? [...row,d.promptSnippet ?? null,d.promptGuidelines ?? null] : row;
 });
 const originalLog = console.log, originalNow = Date.now;
 const originalWrite = process.stdout.write;
 const savedEnv = {...process.env};
 if (request.execReceipt && fs.existsSync(request.execReceipt)) fs.unlinkSync(request.execReceipt);
 if (request.notifyEnv) {
  delete process.env.WT_SESSION; delete process.env.KITTY_WINDOW_ID;
  Object.assign(process.env,request.notifyEnv);
  process.stdout.write = ((data: string) => {
   if (request.writeError) throw new Error(request.writeError);
   trace.push(["stdout",data]); return true;
  }) as any;
 }
 const originalTimers = [globalThis.setTimeout,globalThis.clearTimeout,globalThis.setInterval,globalThis.clearInterval];
 if (request.timers) {
  globalThis.setTimeout = ((callback: () => void, ms: number) => { const id = ++timerId; timers.set(id,{callback,at:clock+ms}); return id; }) as any;
  globalThis.setInterval = ((callback: () => void, ms: number) => { const id = ++timerId; timers.set(id,{callback,at:clock+ms,interval:ms}); return id; }) as any;
  globalThis.clearTimeout = globalThis.clearInterval = ((id: number) => timers.delete(id)) as any;
 }
 const originalCwd = process.cwd;
 if (request.processCwd) process.cwd = () => request.processCwd;
 const times = [...(request.times ?? [])];
 if (times.length) Date.now = () => times.shift() ?? originalNow();
 console.log = (...args: any[]) => {
  if (request.logError) throw new Error(request.logError);
  trace.push(["log", args.join(" ")]);
 };
 try {
  for (const step of request.steps) {
   switch (step.op) {
    case "idle": idle = step.value; break;
    case "command":
     try { await runner.getCommand(step.name).handler(step.args ?? "", runner.createCommandContext()); }
     catch (error: any) { trace.push(["error",error.message]); }
     break;
    case "event": await runner.emit(step.event); break;
    case "emit": trace.push(["result",await runner.emit(step.event) ?? null]); break;
    case "advance": advance(step.ms); break;
    case "name": name = step.value; break;
    case "receipt": {
     const bytes = await new Promise<Buffer>((done,reject) => {
      const timeout = originalTimers[0](() => { watcher.close(); reject(new Error("command fixture timeout")); },3000);
      const check = () => {
       if (!fs.existsSync(request.execReceipt)) return;
       const bytes = fileOps.readFileSync(request.execReceipt);
       if (!bytes.length) return;
       originalTimers[1](timeout); watcher.close(); done(bytes);
      };
      const watcher = fileOps.watch(dirname(request.execReceipt),check); check();
     });
     trace.push(["exec",JSON.parse(bytes.toString())]); break;
    }
    case "trigger": {
     await new Promise<void>((done,reject) => {
      const timeout = originalTimers[0](() => reject(new Error("trigger fixture timeout")),3000);
      fileChanged = () => { originalTimers[1](timeout); fileChanged = undefined; done(); };
      const fd = fs.openSync(request.triggerPath,"r+");
      fs.writeSync(fd,step.content); fs.closeSync(fd);
     });
     trace.push(["file",fileOps.readFileSync(request.triggerPath,"utf8")]);
     break;
    }
    case "bus": bus.emit(step.channel, step.data); break;
    case "prompt": prompt = step.value; break;
    case "before": {
     const result = await runner.emitBeforeAgentStart("task", undefined, step.value, {});
     trace.push(["before",result ?? null]); break;
    }
    case "append": manager.appendMessage(step.message); break;
    case "label": manager.appendLabelChange(manager.getEntries()[step.index].id,step.value ?? undefined); break;
    case "renderEntry": {
     const component = extension.entryRenderers.get(step.entry.customType)(step.entry, {expanded:step.expanded}, theme);
     trace.push(["renderEntry",component.render(step.width)]); break;
    }
    case "renderMessage": {
     const component = extension.messageRenderers.get(step.message.customType)(step.message, {expanded:step.expanded,outputPad:step.pad}, theme);
     trace.push(["renderMessage",component.render(step.width)]); break;
    }
    case "renderTool": {
     const component = extension.tools.get(step.name).definition.renderResult(step.result, {expanded:true,isPartial:false}, theme);
     trace.push(["renderTool",component.render(step.width)]); break;
    }
    case "footer": footer.invalidate(); trace.push(["footer",footer.render(step.width)]); break;
    case "branchChange": branch = step.name; branchCallback?.(); break;
    case "tool": {
     const tool = extension.tools.get(step.name).definition;
     try {
      const update = request.updates ? (result: any) => trace.push(["update",result]) : undefined;
      const result = await tool.execute("fixture-call",step.params,undefined,update,runner.createContext());
      trace.push(["tool",result]);
     } catch (error: any) { trace.push(["error",error.message]); }
     break;
    }
    default: throw new Error(`Unknown step ${step.op}`);
   }
  }
 } finally {
  console.log = originalLog; Date.now = originalNow; process.cwd = originalCwd;
  process.stdout.write = originalWrite;
  if (request.notifyEnv) { for (const key of Object.keys(process.env)) if (!(key in savedEnv)) delete process.env[key]; Object.assign(process.env,savedEnv); }
  for (const watcher of watchers) watcher.close();
  Object.assign(fs,fileOps); syncBuiltinESMExports();
  [globalThis.setTimeout,globalThis.clearTimeout,globalThis.setInterval,globalThis.clearInterval] = originalTimers as any;
 }
 return request.observeBus ? {commands,tools,trace,observed} : {commands,tools,trace};
}
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
if (pending) throw new Error("Unterminated oracle request");
}
main().catch(error => { console.error(error); process.exit(1); });
