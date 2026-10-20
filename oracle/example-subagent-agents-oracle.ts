import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
async function main() {
const module = await import(pathToFileURL(process.argv[2]).href);
for await (const line of createInterface({input:process.stdin})) {
 const r = JSON.parse(line);
 try {
  const discovery = module.discoverAgents(r.cwd,r.scope);
  console.log(JSON.stringify({discovery,lists:r.limits.map((n:number)=>module.formatAgentList(discovery.agents,n))}));
 } catch { console.log(JSON.stringify({failed:true})); }
}
}
main().catch(e=>{console.error(e);process.exit(1)});
