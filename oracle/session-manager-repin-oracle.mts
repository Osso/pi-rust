import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import type * as Source from "@pi-oracle-source/coding-agent/src/core/session-manager.ts";

type Api = typeof Source;
type Manager = InstanceType<Api["SessionManager"]>;
const api = (await import(pathToFileURL(process.argv[2]).href)) as Api;

type Request = {
 id: number;
 mode: "open" | "memory" | "create" | "fork" | "load";
 path: string;
 cwd: string;
 dir?: string;
 sessionId?: string;
 from?: string | null;
 commands?: Command[];
};
type Command = {
 op: string;
 text?: string;
 target?: string;
 path?: string;
 value?: string;
};

function snapshot(manager: Manager) {
 const path = manager.getSessionFile();
 let bytes: string | null = null;
 if (path) {
  try { bytes = readFileSync(path, "utf8"); } catch { /* An unflushed session has no file. */ }
 }
 return {
  header: manager.getHeader(), entries: manager.getEntries(), tree: manager.getTree(),
  context: manager.buildSessionContext(), contextEntries: manager.buildContextEntries(),
  cwd: manager.getCwd(), leaf: manager.getLeafId(), name: manager.getSessionName() ?? null,
  nameState: manager.hasSessionNameState(), persisted: manager.isPersisted(), bytes,
 };
}

function apply(manager: Manager, command: Command) {
 const text = command.text ?? "";
 const target = command.target ?? "";
 switch (command.op) {
  case "user": return manager.appendMessage({ role: "user", content: text, timestamp: 1 });
  case "custom": return manager.appendCustomEntry("state", { text, nested: { n: 2 } });
  case "thinking": return manager.appendThinkingLevelChange(text);
  case "model": return manager.appendModelChange("anthropic", text);
  case "label": return manager.appendLabelChange(target, command.text);
  case "branch": return manager.branch(target);
  case "summary": return manager.branchWithSummary(command.target ?? null, text, { source: "repin" }, false);
  case "reset": return manager.resetLeaf();
  case "persist": return manager.persistForRecovery();
  case "sandbox": return manager.setSessionSandboxProfile(undefined);
  case "db": return manager.setMetadataControlDbPath(command.path);
  case "goal": return manager.getSessionGoalJsonForSession(command.path ?? "");
  case "setGoal": return manager.setSessionGoalJson(text);
  case "name": return manager.setSessionName(command.text);
  case "switch": return manager.setSessionFile(command.path ?? "");
  case "compaction": return manager.appendCompaction(text, target, 123, { source: "repin" }, false, 42);
  case "update": return manager.updateCompactionSummary(target, text);
  case "extract": return manager.createBranchedSession(target);
  case "remove": return rmSync(manager.getSessionFile()!, { force: true });
  case "directory": {
   const path = manager.getSessionFile()!;
   rmSync(path, { force: true }); mkdirSync(path); return;
  }
  case "write": return writeFileSync(command.path!, command.value!);
  default: throw new Error(`Unknown command ${command.op}`);
 }
}

for await (const line of createInterface({ input: process.stdin })) {
 const request = JSON.parse(line) as Request;
 let manager: Manager | undefined;
 try {
  if (request.mode === "load") {
   process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output: api.loadEntriesFromFile(request.path) })}\n`);
   continue;
  }
  const options = request.sessionId === undefined ? undefined : { id: request.sessionId };
  manager = request.mode === "memory" ? api.SessionManager.inMemory(request.cwd, options)
   : request.mode === "create" ? api.SessionManager.create(request.cwd, request.dir, options)
   : request.mode === "fork" ? api.SessionManager.forkFrom(request.path, request.cwd, request.dir, options, request.from)
   : api.SessionManager.open(request.path, request.dir);
  const results = [];
  for (const command of request.commands ?? []) results.push(apply(manager, command) ?? null);
  process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output: { results, state: snapshot(manager) } })}\n`);
 } catch (error) {
  if (!(error instanceof Error)) throw error;
  process.stdout.write(`${JSON.stringify({ id: request.id, ok: false, error: { name: error.name, message: error.message, ...Object.fromEntries(Object.entries(error).filter(([key]) => key !== "stack")) }, state: manager ? snapshot(manager) : null })}\n`);
 }
}
