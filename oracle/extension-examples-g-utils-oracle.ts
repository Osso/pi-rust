// Uses unchanged pinned pure helpers; process isolation inherited from cargo's runner.
import { pathToFileURL } from "node:url";
async function main() {
 const utils = await import(pathToFileURL(process.argv[2]).href);
 process.stdin.setEncoding("utf8");
 let pending = "";
 for await (const chunk of process.stdin) {
  pending += chunk;
  let end: number;
  while ((end = pending.indexOf("\n")) !== -1) {
   const request = JSON.parse(pending.slice(0,end)); pending = pending.slice(end+1);
   const result = request.map((item: any) => {
    const todos = structuredClone(item.todos ?? []);
    return JSON.stringify({safe:utils.isSafeCommand(item.text),clean:utils.cleanStepText(item.text),
     todos:utils.extractTodoItems(item.text),done:utils.extractDoneSteps(item.text),
     marked:utils.markCompletedSteps(item.text,todos),updated:todos});
   });
   console.log(JSON.stringify(result));
  }
 }
 if (pending) throw new Error("Unterminated oracle request");
}
main().catch(error => { console.error(error); process.exitCode = 1; });
