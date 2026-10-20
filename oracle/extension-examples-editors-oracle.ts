import { registerHooks } from "node:module";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

const source = process.argv[2];
const packages = source.slice(0, source.indexOf("/packages/") + "/packages".length);
registerHooks({
 resolve(specifier, context, next) {
  if (specifier === "@earendil-works/pi-coding-agent") {
   return { url: pathToFileURL(`${packages}/coding-agent/src/modes/interactive/components/custom-editor.ts`).href, shortCircuit: true };
  }
  return next(specifier, context);
 },
});

runOracle(async (api, request) => {
 const tuiApi = await import(pathToFileURL(`${packages}/tui/src/index.ts`).href);
 const kbApi = await import(pathToFileURL(`${packages}/coding-agent/src/core/keybindings.ts`).href);
 const timers = new Map<number, { callback: () => void; ms: number; elapsed: number }>();
 let nextTimer = 0;
 const originalSet = globalThis.setInterval;
 const originalClear = globalThis.clearInterval;
 globalThis.setInterval = ((callback: () => void, ms: number) => {
  const id = ++nextTimer; timers.set(id, { callback, ms, elapsed: 0 }); return id;
 }) as typeof setInterval;
 globalThis.clearInterval = ((id: number) => { timers.delete(id); }) as typeof clearInterval;
 const handlers = new Map<string, Function>();
 const commands = new Map<string, any>();
 const effects: unknown[] = [];
 const renders: unknown[] = [];
 let editor: any;
 let component: any;
 let result: unknown = null;
 const theme = { fg(role: string, text: string) { return `\x1b[38;2;${request.colors[role].join(";")}m${text}\x1b[39m`; } };
 const terminal = { rows: 24, columns: 80, write() {}, hideCursor() {}, showCursor() {} };
 const tui = new tuiApi.TUI(terminal, true);
 const editorTheme = { borderColor: (s: string) => s, selectList: { selectedPrefix: (s: string) => s, selectedText: (s: string) => s, description: (s: string) => s, scrollInfo: (s: string) => s, noMatch: (s: string) => s } };
 const kb = new kbApi.KeybindingsManager({});
 const live = { ...request };
 const ctx = { cwd: request.cwd, model: request.model, getContextUsage: () => live.usage, ui: {
  theme,
  setWorkingVisible(value: boolean) { effects.push(["workingVisible", value]); },
  setFooter(factory: Function) { effects.push(["footer", factory(tui, theme).render(80)]); },
  setEditorComponent(factory: Function) { editor = factory(tui, editorTheme, kb); if (request.wireClear) editor.onAction("app.clear", () => { editor.setText(""); }); tui.setFocus(editor); effects.push(["editor"]); },
  notify(message: string, level: string) { effects.push(["notify", message, level]); },
  async custom(factory: Function, options: unknown) {
   effects.push(["custom", options]);
   component = factory(tui, theme, kb, (value: unknown) => { result = value ?? null; });
   tui.showOverlay(component);
   drive(component, false);
   tui.hideOverlay();
   return result;
  },
 } };
 function snapshot(target: any, isEditor: boolean) {
  renders.push({ text: isEditor ? target.getText() : null, focused: target.focused ?? null, lines: request.widths.map((w: number) => target.render(w)) });
 }
 function drive(target: any, isEditor: boolean) {
  snapshot(target, isEditor);
  for (const step of request.steps) {
   for (const key of ["usage", "thinking"]) if (key in step) live[key] = step[key];
   if ("model" in step) ctx.model = step.model;
   if (step.input !== undefined) target.handleInput(step.input);
   if (step.focus !== undefined) tui.setFocus(step.focus ? target : null);
   if (step.event !== undefined) handlers.get(step.event)?.({}, ctx);
   if (step.tick !== undefined) for (const timer of [...timers.values()]) {
    timer.elapsed += step.tick;
    while (timer.elapsed >= timer.ms && [...timers.values()].includes(timer)) { timer.elapsed -= timer.ms; timer.callback(); }
   }
   snapshot(target, isEditor);
  }
 }
 try {
  api.default({ on(name: string, handler: Function) { handlers.set(name, handler); }, registerCommand(name: string, command: any) { commands.set(name, command); }, getThinkingLevel() { return live.thinking ?? "off"; }, async exec() { return {stdout: request.branch ?? "", stderr: "", code: 0}; } });
  if (commands.has("overlay-test")) await commands.get("overlay-test").handler("", ctx);
  else { await handlers.get("session_start")!({}, ctx); await Promise.resolve(); drive(editor, true); }
  return { effects, renders, result };
 } finally {
  globalThis.setInterval = originalSet;
  globalThis.clearInterval = originalClear;
 }
}).catch(error => { console.error(error); process.exitCode = 1; });
