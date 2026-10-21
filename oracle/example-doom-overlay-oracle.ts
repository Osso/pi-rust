// Complete pinned component/index programs; only the external engine, WAD
// acquisition and timer clock are fixtures. Actual pinned key mapper/TUI remain.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import vm from "node:vm";
const source = process.argv[2];
const require = createRequire(source);
const ts = require("typescript");
function evaluate(file: string, imports: Record<string, unknown>, globals: Record<string, unknown>) {
 const javascript = ts.transpileModule(readFileSync(file,"utf8"), {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const exports = {};
 vm.runInNewContext(javascript, {exports, require:(name: string)=> {
  if (!(name in imports)) throw new Error(`Unexpected import ${name}`);
  return imports[name];
 }, ...globals});
 return exports as any;
}
async function run(r: any) {
 const trace: any[] = [];
 const keys = await import(pathToFileURL(join(dirname(source),"doom-keys.ts")).href);
 const tui = await import(pathToFileURL(join(dirname(source),"../../../../tui/src/index.ts")).href);
 tui.setKittyProtocolActive(false);
 let current: any = r, timer: (()=>void) | null = null, keyCalls = 0, clock = 0, nextTick = 0;
 class Engine {
  width = 2; height = 2;
  constructor(public wad: string) { if (r.kind === "index") trace.push(["new",wad]); }
  async init() {trace.push(["init",this.wad]); if (current.initError) throw new Error(current.initError);}
  tick() {trace.push(["tick"]);if(current.tickError)throw new Error("fixture tick failed");}
  pushKey(pressed: boolean,key: number) {trace.push(["key",pressed?1:0,key]);keyCalls++;if(current.keyErrorAt === keyCalls)throw new Error("fixture key failed");}
  getFrameRGBA() {return Uint8Array.from([255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255]);}
 }
 const components = evaluate(join(dirname(source),"doom-component.ts"), {
  "@earendil-works/pi-tui":tui, "./doom-keys.ts":keys,
 }, {
  setInterval(callback: ()=>void, ms: number) {if(ms !== 1000/35)throw new Error("wrong timer");timer=callback;nextTick=clock+28;return 1;},
  clearInterval() {timer=null;},
 });
 const context = {requestRender() {trace.push(["requestRender"]);if(current.renderError)throw new Error("fixture render request failed");}};
 function drive(component: any, steps: any[]) {
  trace.push(["wantsKeyRelease",component.wantsKeyRelease]);
  for (const step of steps) {
   if (step.render !== undefined) trace.push(["render",step.render,component.render(step.render)]);
   if (step.input !== undefined) {
    const input=step.units?String.fromCharCode(...step.units).toWellFormed():step.input;
    try {component.handleInput(input);}catch(e:any){trace.push(["uncaught",`${e.name}: ${e.message}`]);}
   }
   if (step.ms !== undefined) {
    clock+=step.ms;
    while(timer && clock>=nextTick) {nextTick+=28;timer();}
   }
   if (step.invalidate) component.invalidate();
   if (step.dispose) component.dispose();
  }
  component.dispose();
 }
 if (r.kind === "component") {
  try {
   const component = new components.DoomOverlayComponent(context,new Engine("fixture"),()=>trace.push(["done",null]),r.resume ?? false);
   drive(component,r.steps);
  } catch(e:any) {trace.push(["constructorError",`${e.name}: ${e.message}`]);}
  if(timer) {timer();trace.push(["leakedTimer"]);}
  return trace;
 }
 const index = evaluate(source, {
  "./doom-component.ts":components,"./doom-engine.ts":{DoomEngine:Engine},
  "./wad-finder.ts":{ensureWadFile:async()=>{trace.push(["ensureWad"]);return current.wad ?? null;}},
 }, {});
 let command: any;
 index.default({registerCommand(name: string, options: any) {trace.push(["command",name,options.description]);command=options;}});
 for (const invocation of r.commands) {
  current=invocation;keyCalls=0;
  await command.handler(invocation.args ?? "", {mode:invocation.mode ?? "tui",ui:{
   notify(message: string, level: string) {trace.push(["notify",message,level]);},
   async custom(factory: any, options: any) {
    trace.push(["custom",options]);
    if(invocation.customError)throw new Error(invocation.customError);
    let value:any;
    const component=factory(context,{}, {}, (result:any)=>{value=result;trace.push(["done",result ?? null]);});
    drive(component,invocation.steps ?? [{input:"q"}]);
    return value;
   },
  }});
 }
 return trace;
}
async function main() {
 process.stdin.setEncoding("utf8");let pending="";
 for await(const chunk of process.stdin) {
  pending+=chunk;let end:number;
  while((end=pending.indexOf("\n"))!==-1) {
   const line=pending.slice(0,end);pending=pending.slice(end+1);
   console.log(JSON.stringify(await run(JSON.parse(line))));
  }
 }
}
main().catch(error=>{console.error(error);process.exit(1);});
