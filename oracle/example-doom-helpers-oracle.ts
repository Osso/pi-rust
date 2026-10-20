// Executes complete pinned helper modules. Only the waived Emscripten asset and
// network/module-relative location seams are substituted; filesystem IO is real.
import { readFileSync, existsSync, writeFileSync, unlinkSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";
import * as fs from "node:fs";
import * as path from "node:path";
import * as url from "node:url";
import * as zlib from "node:zlib";
import vm from "node:vm";
const sourcePath = process.argv[2];
const require = createRequire(sourcePath);
const ts = require("typescript");
function evaluate(file: string, overrides: any, globals: any = {}) {
 const source = readFileSync(file,"utf8").replaceAll("import.meta.url", JSON.stringify(overrides.sourceUrl ?? pathToFileURL(file).href));
 const exports = {};
 const javascript = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const context = vm.createContext({exports,Buffer,Uint8Array,Uint32Array,process:overrides.process ?? process, ...globals,
  require:(name:string) => {
   if (name === "node:fs") return overrides.fs ?? fs;
   if (name === "node:path") return path;
   if (name === "node:url") return url;
   if (name === "node:zlib") return zlib;
   if (name === "node:module") return {createRequire:()=>require};
   throw new Error(`Unexpected import ${name}`);
  }});
 vm.runInContext(javascript,context);
 return exports as any;
}
async function run(r: any) {
 if (r.kind === "gzip") return [...gzipSync(Buffer.from(r.bytes))];
 if (r.kind === "keys") {
  const keys = await import(pathToFileURL(sourcePath).href);
  const tui = await import(pathToFileURL(join(dirname(sourcePath),"../../../../tui/src/index.ts")).href);
  tui.setKittyProtocolActive(r.kitty);
  return {constants:Object.entries(keys.DoomKeys),mapped:r.inputs.map((input:string)=>keys.mapKeyToDoom(input))};
 }
 if (r.kind === "engine") {
  const trace:any[]=[];
  let next=64;
  const fixtureModule = {
   FS_createPath:(p:string,n:string,a:boolean,b:boolean)=>{trace.push(["path",p,n,a,b]);},
   FS_createDataFile:(p:string,n:string,d:number[],a:boolean,b:boolean)=>{trace.push(["file",p,n,d,a,b]);},
   _malloc:(size:number)=>{const ptr=next;next+=size;trace.push(["malloc",size,ptr]);return ptr;},
   setValue:(ptr:number,v:number,k:string)=>{trace.push(["set",ptr,v,k]);},
   _doomgeneric_Create:(argc:number,argv:number)=>{trace.push(["create",argc,argv]);if(r.fail)throw new Error("fixture create failed");},
   _free:(ptr:number)=>trace.push(["free",ptr]),
   _DG_GetFrameBuffer:()=>1024,_DG_GetScreenWidth:()=>2,_DG_GetScreenHeight:()=>1,
   getValue:(ptr:number)=>ptr===1024?0x12345678:0xffabcdef|0,
   _doomgeneric_Tick:()=>trace.push(["tick"]),
   _DG_PushKeyEvent:(pressed:number,key:number)=>trace.push(["key",pressed,key]),
  };
  const exports=evaluate(sourcePath,{fs:{...fs,existsSync:()=>true,readFileSync:(p:string,encoding?:string)=>p.endsWith("doom.js")?'module.exports = async config => { config.preRun.forEach(f => f(fixtureModule)); return fixtureModule; };':readFileSync(p,encoding as any)}},{fixtureModule});
  const engine=new exports.DoomEngine(r.wad);
  engine.tick();engine.pushKey(true,173);
  let error=null;
  try {await engine.init();} catch(e:any) {error=e.message;}
  engine.tick();engine.pushKey(true,173);engine.pushKey(false,173);
  return {trace,error,initialized:engine.isInitialized(),width:engine.width,height:engine.height,frame:[...engine.getFrameRGBA()]};
 }
 const calls:any[]=[];
 const bundled=join(r.bundled,"doom1.wad");
 const exports=evaluate(sourcePath,{sourceUrl:pathToFileURL(join(r.bundled,"wad-finder.ts")).href,
  process:{cwd:()=>r.cwd,env:{HOME:r.home ?? ""}}}, {
  fetch:async (address:string)=>{
   calls.push(["GET",address]);
   if(r.fail)throw new Error("fixture network failed");
   return {ok:r.status>=200 && r.status<300,status:r.status,arrayBuffer:async()=>{if(r.bodyFail)throw new Error("fixture body failed");return Uint8Array.from(r.bytes).buffer;}};
  }
 });
 if(r.kind === "wad-find") return exports.findWadFile(r.custom ?? undefined);
 const before=existsSync(bundled)?readFileSync(bundled):null;
 const result=await exports.ensureWadFile();
 const bytes=existsSync(bundled)?[...readFileSync(bundled)]:null;
 if(before)writeFileSync(bundled,before);else if(existsSync(bundled))unlinkSync(bundled);
 return {path:result,calls,bytes};
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
 if(pending.length)throw new Error("Unterminated oracle request");
}
main().catch(error=>{console.error(error);process.exit(1);});
