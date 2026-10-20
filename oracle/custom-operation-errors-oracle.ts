import { pathToFileURL } from "node:url";
import { dirname, join } from "node:path";
async function run(request: any) {
 const module = await import(pathToFileURL(join(dirname(process.argv[2]), `${request.tool}.ts`)).href);
 const fault = (stage: string, value: any) => async () => {
  if (request.stage === stage) throw new Error(request.message ?? "SSH failed (7): scripted failure\n");
  return value;
 };
 const operations = {
  access: fault("access", undefined), readFile: fault(request.stage === "image" ? "image" : "readFile", Buffer.from("old")),
  detectImageMimeType: fault("mime", request.stage === "image" ? "image/png" : null), stat: fault("stat", {size:100000,isDirectory:()=>true}),
  mkdir: fault("mkdir", undefined), writeFile: fault("writeFile", undefined),
  exists: async()=>request.stage !== "missing", readdir: fault("readdir", []),
  isDirectory: fault("directory", true), glob:async()=>[],
  exec: fault(request.stage, {exitCode:0}),
  ...(request.stage === "range" ? {readRange:fault("range", Buffer.alloc(0))}:{}),
 };
 const create = module[`create${request.tool[0].toUpperCase()+request.tool.slice(1)}Tool`];
 try { await create("/fixture", {operations}).execute("fault", request.params); return {error:null}; }
 catch(error:any) {return {error:error.message};}
}
async function main() {
let pending="";
process.stdin.setEncoding("utf8");
for await(const chunk of process.stdin) {
 pending+=chunk;
 let end:number;
 while((end=pending.indexOf("\n"))!==-1) {
  const line=pending.slice(0,end); pending=pending.slice(end+1);
  console.log(JSON.stringify(await run(JSON.parse(line))));
 }
}
}
main().catch(error=>{console.error(error);process.exit(1);});
