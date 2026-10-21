// Whole-module oracle. Imports only the unmodified lock-selected source.
import { spawn, type ChildProcess } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, readdirSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
const source = process.argv[2];
const resident = await import(pathToFileURL(`${source}/core/resident-session.ts`).href);
const db = await import(pathToFileURL(`${source}/core/session-control-db.ts`).href);
const owned: ChildProcess[] = [];

type Entry = { path: string; kind?: string; target?: string };
type Fixture = {
	root: string; agentDir: string; role: "architect" | "supervisor"; db: string;
	entries: Entry[]; metadata: { sessionPath: string; id: string }[];
	listener?: { path?: string; agentId?: string; process: "self" | "dead" | "nonpi" };
	mode?: number;
};

function metadata(row: { sessionPath: string; id: string }): object {
	return { ...row, cwd: "/fixture", createdAt: "2026-10-21T00:00:00.000Z", modifiedAt: "2026-10-21T00:00:00.000Z", messageCount: 0, firstMessage: "" };
}

async function seed(f: Fixture): Promise<void> {
	for (const entry of f.entries) {
		mkdirSync(dirname(entry.path), { recursive: true });
		if (entry.kind === "dir") mkdirSync(entry.path, { recursive: true });
		else if (entry.kind === "symlink") symlinkSync(entry.target!, entry.path);
		else writeFileSync(entry.path, "fixture\n");
	}
	for (const row of f.metadata) db.writeSessionMetadata(f.db, metadata(row));
	if (f.listener) {
		let pid = process.pid;
		if (f.listener.process !== "self") {
			const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
			owned.push(child);
			await new Promise<void>((resolve, reject) => { child.once("spawn", resolve); child.once("error", reject); });
			pid = child.pid!;
			if (f.listener.process === "dead") {
				const exited = new Promise<void>((resolve) => child.once("exit", () => resolve()));
				child.kill();
				await exited;
			}
		}
		db.registerRuntimeMailboxListener(f.db, { sessionId: f.role, agentId: f.listener.agentId ?? null }, pid, f.listener.path,
			{ reconcileRuntimeReplacement: false });
	}
	if (f.mode !== undefined) chmodSync(join(f.agentDir, `${f.role}-sessions`), f.mode);
}

function inspect(f: Fixture): object {
	const directory = join(f.agentDir, `${f.role}-sessions`);
	return {
		entries: existsSync(directory) ? readdirSync(directory, { withFileTypes: true }).map((entry) => ({ name: entry.name, file: entry.isFile(), directory: entry.isDirectory(), symlink: entry.isSymbolicLink() })) : [],
		metadata: db.listSessionMetadata(f.db).map((row: { id: string; sessionPath: string }) => ({ id: row.id, sessionPath: row.sessionPath })).sort((a: { sessionPath: string }, b: { sessionPath: string }) => a.sessionPath < b.sessionPath ? -1 : a.sessionPath > b.sessionPath ? 1 : 0),
	};
}

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	try {
		let value: unknown;
		if (request.op === "seed") value = await seed(request.fixture);
		else if (request.op === "inspect") value = inspect(request.fixture);
		else if (request.op === "prepare") value = resident.prepareResidentSessionFile(request.fixture.agentDir, request.fixture.role, request.fixture.db);
		else if (request.op === "predicate") value = resident.isResidentSession(request.session);
		else throw new Error(`Unknown resident oracle operation ${request.op}`);
		console.log(JSON.stringify({ ok: true, value: value ?? null }));
	} catch (error) {
		const e = error as NodeJS.ErrnoException;
		const messageUnits = e.name === "TypeError" ? Array.from({ length: e.message.length }, (_, index) => e.message.charCodeAt(index)) : undefined;
		console.log(JSON.stringify({ ok: false, error: { name: e.name, message: e.message.toWellFormed(), code: e.code, errno: e.errno, syscall: e.syscall, path: e.path }, messageUnits }));
	}
}
for (const child of owned) if (child.exitCode === null && child.signalCode === null) child.kill();

}
void main().catch((error) => { console.error(error); process.exitCode = 1; });
