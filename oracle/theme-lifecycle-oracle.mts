import { createInterface } from "node:readline";
import { writeFileSync, renameSync, rmSync } from "node:fs";
import { pathToFileURL } from "node:url";
const api = await import(pathToFileURL(process.argv[2]!).href);
const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
for await (const line of createInterface({input:process.stdin})) {
 const request = JSON.parse(line);
 try {
  for (const [file,value] of Object.entries(request.setup)) writeFileSync(file, JSON.stringify(value));
  api.stopThemeWatcher(); api.setRegisteredThemes([]);
  const callbacks: string[] = [];
  api.onThemeChange(() => callbacks.push(api.theme.getFgAnsi("accent")));
  api.initTheme("watch",true);
  const frames: unknown[] = [];
  for (const step of request.steps) {
   switch(step.op) {
    case "write": writeFileSync(step.path,step.raw ?? JSON.stringify(step.value));break;
    case "rename": renameSync(step.from,step.to);break;
    case "remove": rmSync(step.path);break;
    case "wait": await sleep(step.ms);break;
    case "set": api.setTheme(step.name,true);break;
    case "stop": api.stopThemeWatcher();break;
    case "snapshot": {
     const beforeAccess = [...callbacks];
     frames.push({callbacks:beforeAccess,name:api.theme.name,accent:api.theme.getFgAnsi("accent"),registered:api.getThemeByName("watch")?.getFgAnsi("accent")});
     break;
    }
    default:throw new Error(`Unknown operation ${step.op}`);
   }
  }
  api.stopThemeWatcher();
  console.log(JSON.stringify({id:request.id,ok:true,output:frames}));
 } catch(error) {api.stopThemeWatcher();console.log(JSON.stringify({id:request.id,ok:false,error:String(error)}));}
}
