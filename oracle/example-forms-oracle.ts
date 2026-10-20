import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";
const source = process.argv[2];
const packages = source.slice(0, source.indexOf("/packages/") + "/packages".length);
runOracle(async (api, request) => {
 const tuiApi = await import(pathToFileURL(`${packages}/tui/src/index.ts`).href);
 const kbApi = await import(pathToFileURL(`${packages}/coding-agent/src/core/keybindings.ts`).href);
 let tool: any;
 api.default({registerTool(value: any) {tool = value;}});
 const theme = {
  fg(role: string, text: string) {return `\x1b[38;2;${request.colors[role].join(";")}m${text}\x1b[39m`;},
  bg(role: string, text: string) {return `\x1b[48;2;${request.colors[role].join(";")}m${text}\x1b[49m`;},
  bold(text: string) {return `\x1b[1m${text}\x1b[22m`;},
 };
 const terminal = {rows:24, columns:80, write() {}, hideCursor() {}, showCursor() {}};
 const tui = new tuiApi.TUI(terminal, true);
 const renders: unknown[] = [];
 const ctx = {mode: request.mode ?? "tui", ui: {async custom(factory: Function) {
  let result: unknown = null;
  let completed = false;
  const component = factory(tui, theme, new kbApi.KeybindingsManager({}), (value: unknown) => {result=value;completed=true;});
  const record = () => renders.push(request.widths.map((width: number) => component.render(width)));
  record();
  for(const step of request.steps ?? []) {
   if(completed) break;
   if(step.invalidate) component.invalidate();
   if(step.input !== undefined) component.handleInput(step.input);
   if(step.inputUnits) component.handleInput(String.fromCharCode(...step.inputUnits));
   record();
  }
  return result;
 }}};
 const result = await tool.execute("fixture", request.params, undefined, undefined, ctx);
 const renderCall = tool.renderCall(request.params, theme, {}).render(request.callWidth ?? 80);
 const renderResult = tool.renderResult(result, {}, theme, {}).render(request.callWidth ?? 80);
 const fallback = tool.renderResult({content:[{type:"text",text:"fallback"}]}, {}, theme, {}).render(80);
 const metadata = {name:tool.name,label:tool.label,description:tool.description,parameters:tool.parameters,executionMode:tool.executionMode ?? null};
 function wellFormed(value: any): any {
  if(typeof value === "string") return value.toWellFormed();
  if(Array.isArray(value)) return value.map(wellFormed);
  if(value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,wellFormed(v)]));
  return value;
 }
 return wellFormed({metadata,result,renders,renderCall,renderResult,fallback});
}).catch(error => {console.error(error);process.exitCode=1;});
