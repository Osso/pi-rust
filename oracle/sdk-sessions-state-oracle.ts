import { pathToFileURL } from "node:url";
// Real pinned SessionManager: no SDK stubs. Each operation is independent.
async function main() {
 const { SessionManager } = await import(pathToFileURL(process.argv[2]).href);
 let pending = "";
 process.stdin.setEncoding("utf8");
 for await (const chunk of process.stdin) {
  pending += chunk;
  let end: number;
  while ((end = pending.indexOf("\n")) !== -1) {
   const request = JSON.parse(pending.slice(0, end));
   pending = pending.slice(end + 1);
   let result;
   if (request.operation === "continue") {
    const manager = SessionManager.continueRecent(request.cwd, request.dir);
    result = {path: manager.getSessionFile()};
   } else if (request.operation === "list") {
    const sessions = await SessionManager.list(request.cwd, request.dir, undefined, request.db);
    result = sessions.map((s: any) => ({path:s.path,id:s.id,firstMessage:s.firstMessage}));
   } else throw new Error(`Unknown operation ${request.operation}`);
   console.log(JSON.stringify(result));
  }
 }
 if (pending.length) throw new Error("Unterminated oracle request");
}
main().catch(error => {console.error(error);process.exit(1);});
