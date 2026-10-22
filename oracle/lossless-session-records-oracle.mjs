import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

// Set private state roots before importing the pinned module. Never open live state.
const sandbox = mkdtempSync(join(tmpdir(), "pi-q18-record-oracle-"));
process.env.HOME = sandbox;
process.env.PI_CODING_AGENT_DIR = join(sandbox, "agent");
process.env.PI_CONTROL_DB = join(sandbox, "control.sqlite");
const modulePath = process.argv[2];
const manager = await import(pathToFileURL(modulePath).href);
const output = await import(pathToFileURL(join(dirname(modulePath), "session-tool-output.ts")).href);

function snapshot(session) {
	return {
		header: JSON.stringify(session.getHeader()),
		entries: session.getEntries().map((entry) => JSON.stringify(entry)),
		context: JSON.stringify(session.buildSessionContext()),
		tree: JSON.stringify(session.getTree()),
		id: session.getSessionId(),
		leafId: session.getLeafId(),
	};
}

function handle(request) {
	switch (request.operation) {
		case "parse":
			return manager.parseSessionEntries(request.content).map((entry) => JSON.stringify(entry));
		case "migrate": {
			const entries = manager.parseSessionEntries(request.content);
			manager.migrateSessionEntries(entries);
			return entries.map((entry) => JSON.stringify(entry));
		}
		case "serialize":
			return manager.parseSessionEntries(request.content).map(output.serializeSessionEntryForPersistence);
		case "write":
			writeFileSync(request.path, request.content);
			return readFileSync(request.path, "utf8");
		case "read":
			return snapshot(manager.SessionManager.open(request.path, request.sessionDir));
		default:
			throw new Error(`Unknown record oracle operation: ${request.operation}`);
	}
}

try {
	for await (const line of createInterface({ input: process.stdin })) {
		if (!line.trim()) continue;
		const request = JSON.parse(line);
		try {
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output: handle(request) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: false, error: String(error) })}\n`);
		}
	}
} finally {
	rmSync(sandbox, { recursive: true, force: true });
}
