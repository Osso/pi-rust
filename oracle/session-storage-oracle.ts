import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
import { readFileSync, statSync } from "node:fs";
async function main() {
const root = process.argv[2];
const manager = await import(pathToFileURL(join(root, "session-manager.ts")).href);
const archive = await import(pathToFileURL(join(root, "session-archive-storage.ts")).href);
const transcript = await import(pathToFileURL(join(root, "session-transcript.ts")).href);
const history = await import(pathToFileURL(join(root, "session-history.ts")).href);
async function handle(r: any) {
 if (r.op === "create") {
  const m = manager.SessionManager.create(r.cwd, r.dir);
  m.appendMessage({role:"user",content:r.text ?? "hello 😀漢字",timestamp:1}, r.refs);
  m.persistForRecovery();
  return {path:m.getSessionFile(), id:m.getSessionId()};
 }
 if (r.op === "archive") return archive.archivePersistedSession(r.db, r.path);
 if (r.op === "restore") return archive.restoreArchivedSession(r.db, r.path);
 if (r.op === "read") {
  const m = manager.SessionManager.openReadOnly(r.path);
  const entries = []; for await (const e of history.readSessionHistoryReverse(m)) entries.push(e);
  return {id:m.getSessionId(), entries:m.getEntries(), history:entries, header:m.getHeader(), text:transcript.readSessionTranscriptText(r.path)};
 }
 if (r.op === "bytes") return {bytes:readFileSync(r.path).toString("base64"), mode:statSync(r.path).mode & 0o777};
 throw new Error(`Unknown operation: ${r.op}`);
}
for await (const line of createInterface({input:process.stdin})) {
 const r=JSON.parse(line);
 try { console.log(JSON.stringify({id:r.id,ok:true,output:await handle(r)})); }
 catch(e) { console.log(JSON.stringify({id:r.id,ok:false,error:String(e)})); }
}

}
void main();
