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
  if(r.db)m.setMetadataControlDbPath(r.db);
  const first=m.appendMessage({role:"user",content:r.text ?? "hello 😀漢字",timestamp:1}, r.refs);
  if(r.compacted||r.branched){
   const kept=m.appendMessage({role:"user",content:"kept",timestamp:2},r.refs);
   m.branch(first);
   for(let i=0;i<(r.inactiveCount??1);i++)m.appendMessage({role:"user",content:r.largeInactive?"漢😀".repeat(512*1024):"inactive",timestamp:3});
   m.branch(kept);
   if(r.compacted)m.appendCompaction("summary",kept,123);
   m.appendMessage({role:"user",content:"newest",timestamp:4});
  }
  if(r.splitSurrogate)for(const entry of m.getEntries())if(entry.type==="message")entry.mailboxStoreRefs=[{sessionPath:String.fromCharCode(0xd800),messageId:"surrogate"}];
  if(r.splitExtra)for(const entry of m.getEntries())if(entry.type==="compaction"){entry[String.fromCharCode(0xd800)]={value:String.fromCharCode(0xdc00)};entry.usage={[String.fromCharCode(0xdc00)]:String.fromCharCode(0xd800)};}
  if("legacyUsage" in r) for(const entry of m.getEntries())if(entry.type==="compaction")entry.usage=r.legacyUsage;
  if ("rawRefs" in r) for(const entry of m.getEntries()) if(entry.type==="message") entry.mailboxStoreRefs=r.rawRefs;
  m.persistForRecovery();
  return {path:m.getSessionFile(), id:m.getSessionId()};
 }
 if(r.op === "extraTokens") {
  const entry=manager.SessionManager.openReadOnly(r.path).getEntries().find((e:any)=>e.type==="compaction");
  return JSON.stringify([entry[String.fromCharCode(0xd800)],entry.usage]);
 }
 if(r.op === "exportReceiptTokens") {
  const html=readFileSync(r.path,"utf8");const marker='<script id="session-data" type="application/json">';
  const start=html.indexOf(marker)+marker.length;const end=html.indexOf("</script>",start);
  const data=JSON.parse(Buffer.from(html.slice(start,end),"base64").toString("utf8"));
  return JSON.stringify(data.entries.find((e:any)=>e.type==="message").mailboxStoreRefs);
 }
 if(r.op === "receiptTokens") {
  const acceptance=await import(pathToFileURL(join(root,"runtime-mailbox-transcript.ts")).href);
  return JSON.stringify(acceptance.readAcceptedRuntimeMailboxStoreRefs(r.path));
 }
 if(r.op === "scan") {
  const acceptance=await import(pathToFileURL(join(root,"runtime-mailbox-transcript.ts")).href);
  return acceptance.readAcceptedRuntimeMailboxStoreRefs(r.path);
 }
 if (r.op === "archive") return archive.archivePersistedSession(r.db, r.path);
 if (r.op === "restore") return archive.restoreArchivedSession(r.db, r.path);
 if (r.op === "read") {
  const m = manager.SessionManager.openReadOnly(r.path);
  const entries = []; for await (const e of history.readSessionHistoryReverse(m)) entries.push(e);
  return {id:m.getSessionId(), entries:m.getEntries(), history:entries, header:m.getHeader(), tree:m.getTree(), context:m.buildSessionContext().messages, text:transcript.readSessionTranscriptText(r.path)};
 }
 if(r.op === "transcript"){
  let streamed="";if(r.stream!==false)for await(const chunk of transcript.createSessionTranscriptReadStream(r.path))streamed+=chunk;
  return {path:transcript.readSessionTranscriptPath(r.path),isTranscript:transcript.isSessionTranscriptFile(r.path),bytes:transcript.readSessionTranscriptBytes(r.path).toString("base64"),text:transcript.readSessionTranscriptText(r.path),streamed};
 }
 if(r.op === "predicates")return r.paths.map((path:string)=>({transcript:transcript.isSessionTranscriptFile(path),archive:archive.isArchivedSessionFile(path),resolved:transcript.readSessionTranscriptPath(path)}));
 if(r.op === "fileArchive")return archive.archiveSessionFile(r.path);
 if(r.op === "fileRestore")return archive.restoreArchivedSessionFile(r.path);
 if(r.op === "info"){
  const s=await manager.buildSessionInfo(r.path);
  return s?{path:s.path,id:s.id,cwd:s.cwd,messageCount:s.messageCount,firstMessage:s.firstMessage}:null;
 }
 if(r.op === "list")return (await manager.SessionManager.list(r.cwd,r.dir,undefined,r.db)).map((s:any)=>({path:s.path,id:s.id,cwd:s.cwd,messageCount:s.messageCount,firstMessage:s.firstMessage}));
 if(r.op === "metadata"){
  const db=await import(pathToFileURL(join(root,"session-control-db.ts")).href);
  return db.readSessionMetadata(r.db,r.path)??null;
 }
 if(r.op === "mutable")return manager.SessionManager.open(r.path).getSessionId();
 if(r.op === "context")return manager.loadSessionContextEntries(r.path);
 if(r.op === "load")return manager.loadEntriesFromFile(r.path);
 if(r.op === "header")return manager.readSessionHeader(r.path);
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
