// Batch A: unchanged pinned examples, actual loader/runtime/runner/event bus and
// in-memory SessionManager. Only host actions/UI and clock are fixture seams.
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

async function main() {
const file = process.argv[2];
const root = resolve(dirname(file), "../../src");
const load = (path: string) => import(pathToFileURL(resolve(root, path)).href);
const [{default: register}, {createExtensionRuntime, loadExtensionFromFactory}, {ExtensionRunner},
 {createEventBus}, {SessionManager}] = await Promise.all([
 import(pathToFileURL(file).href), load("core/extensions/loader.ts"), load("core/extensions/runner.ts"),
 load("core/event-bus.ts"), load("core/session-manager.ts"),
]);

async function run(request: any) {
 const trace: any[] = [];
 const cwd = request.cwd ?? "/fixture";
 const runtime = createExtensionRuntime(), bus = createEventBus();
 const extension = await loadExtensionFromFactory(register, cwd, bus, runtime, "<example:batch-a>");
 const manager = SessionManager.inMemory("/fixture");
 const runner = new ExtensionRunner([extension], runtime, cwd, manager, {});
 let name: string | undefined = request.name, prompt = "";
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
  setSessionName(value: string) {
   if (request.actionError) throw new Error(request.actionError);
   name = value; trace.push(["setSessionName", value]);
  },
  getSessionName: () => name,
  setLabel(id: string, label?: string) {
   if (request.actionError) throw new Error(request.actionError);
   const index = manager.getEntries().findIndex((entry: any) => entry.id === id);
   manager.appendLabelChange(id, label);
   if (manager.getLabel(id) !== label) throw new Error("label mutation failed");
   trace.push(["setLabel", index, label ?? null]);
  },
 } as any, {getSystemPrompt: () => prompt} as any);
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
  theme: {fg: (role: string, text: string) => {
   const rgb: any = {dim:"90;91;92",accent:"80;180;230",success:"34;204;102"};
   return `\x1b[38;2;${rgb[role]}m${text}\x1b[39m`;
  }},
 } as any, request.mode ?? "tui");
 runner.onError((error: any) => trace.push(["handlerError",error.error]));
 const commands = [...extension.commands.values()].map((c: any) => [c.name, c.description, !!c.getArgumentCompletions]);
 const tools = [...extension.tools.values()].map((t: any) => {
  const d = t.definition; return [d.name, d.label, d.description, d.parameters];
 });
 const originalLog = console.log, originalNow = Date.now;
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
    case "command":
     try { await runner.getCommand(step.name).handler(step.args ?? "", runner.createCommandContext()); }
     catch (error: any) { trace.push(["error",error.message]); }
     break;
    case "event": await runner.emit(step.event); break;
    case "emit": trace.push(["result",await runner.emit(step.event) ?? null]); break;
    case "advance": advance(step.ms); break;
    case "bus": bus.emit(step.channel, step.data); break;
    case "prompt": prompt = step.value; break;
    case "before": {
     const result = await runner.emitBeforeAgentStart("task", undefined, step.value, {});
     trace.push(["before",result?.systemPrompt ?? null]); break;
    }
    case "append": manager.appendMessage(step.message); break;
    case "label": manager.appendLabelChange(manager.getEntries()[step.index].id,step.value ?? undefined); break;
    case "tool": {
     const tool = extension.tools.get(step.name).definition;
     const result = await tool.execute("fixture-call",step.params,undefined,undefined,runner.createContext());
     trace.push(["tool",result]); break;
    }
    default: throw new Error(`Unknown step ${step.op}`);
   }
  }
 } finally {
  console.log = originalLog; Date.now = originalNow; process.cwd = originalCwd;
  [globalThis.setTimeout,globalThis.clearTimeout,globalThis.setInterval,globalThis.clearInterval] = originalTimers as any;
 }
 return {commands,tools,trace};
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
