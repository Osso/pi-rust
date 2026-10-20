import { pathToFileURL } from "node:url";
async function main() {
 const {SettingsManager} = await import(pathToFileURL(process.argv[2]).href);
 let pending = "";
 process.stdin.setEncoding("utf8");
 for await (const chunk of process.stdin) {
  pending += chunk;
  let end: number;
  while ((end = pending.indexOf("\n")) !== -1) {
   const request = JSON.parse(pending.slice(0,end)); pending = pending.slice(end+1);
   const manager = SettingsManager.create(request.cwd, request.agentDir);
   const settings = manager.getGlobalSettings();
   manager.applyOverrides({compaction:{enabled:false},retry:{enabled:true,maxRetries:5,baseDelayMs:1000}});
   manager.setDefaultThinkingLevel("low");
   await manager.flush();
   const errors = manager.drainErrors().map(({scope,error}: any) => ({scope,error:{message:error.message}}));
   console.log(JSON.stringify({settings,errors}));
  }
 }
}
main().catch(error => {console.error(error);process.exit(1);});
