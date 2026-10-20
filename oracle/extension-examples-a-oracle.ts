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
 const runtime = createExtensionRuntime(), bus = createEventBus();
 const extension = await loadExtensionFromFactory(register, "/fixture", bus, runtime, "<example:batch-a>");
 const manager = SessionManager.inMemory("/fixture");
 const runner = new ExtensionRunner([extension], runtime, "/fixture", manager, {});
 let name: string | undefined, prompt = "", idle = true;
 let activeTools: string[] = [];
 runner.bindCore({
  setSessionName(value: string) {
   if (request.actionError) throw new Error(request.actionError);
   name = value; trace.push(["setSessionName", value]);
  },
  getSessionName: () => name,
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
  shutdown: () => trace.push(["shutdown"])} as any);
 runner.bindCommandContext({reload: async () => { trace.push(["reload"]); }} as any);
 runner.setUIContext({
  notify: (message: string, level: string) => trace.push(["notify", message, level]),
  setStatus: (key: string, text?: string) => trace.push(["status", key, text ?? null]),
  theme: {fg: (role: string, text: string) => {
   const rgb: any = {dim:"90;91;92",accent:"80;180;230",success:"34;204;102"};
   return `\x1b[38;2;${rgb[role]}m${text}\x1b[39m`;
  }},
 } as any, "tui");
 runner.onError((error: any) => trace.push(["handlerError",error.error]));
 const commands = [...extension.commands.values()].map((c: any) => [c.name, c.description, !!c.getArgumentCompletions]);
 const tools = [...extension.tools.values()].map((t: any) => {
  const d = t.definition; return [d.name, d.label, d.description, d.parameters];
 });
 const originalLog = console.log, originalNow = Date.now;
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
 } finally { console.log = originalLog; Date.now = originalNow; }
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
