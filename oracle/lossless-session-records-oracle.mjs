import crypto from "node:crypto";
import { syncBuiltinESMExports } from "node:module";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { selectApprovedModule } from "./approved-delta.mjs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

// Set private state roots before importing the pinned module. Never open live state.
const sandbox = mkdtempSync(join(tmpdir(), "pi-q18-record-oracle-"));
process.env.HOME = sandbox;
process.env.PI_CODING_AGENT_DIR = join(sandbox, "agent");
process.env.PI_CONTROL_DB = join(sandbox, "control.sqlite");
const modulePath = selectApprovedModule(
	process.argv[2],
	"packages/coding-agent/src/core/session-manager.ts",
);
const manager = await import(pathToFileURL(modulePath).href);
const output = await import(
	pathToFileURL(join(dirname(modulePath), "session-tool-output.ts")).href
);

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

function migrateWithCollisions(content) {
	const samples = [
		"aaaaaaaa-0000-4000-8000-000000000001",
		"aaaaaaaa-0000-4000-8000-000000000002",
		"bbbbbbbb-0000-4000-8000-000000000003",
	];
	const original = crypto.randomUUID;
	crypto.randomUUID = () => {
		const sample = samples.shift();
		if (!sample) throw new Error("Collision fixture exhausted");
		return sample;
	};
	syncBuiltinESMExports();
	try {
		const entries = manager.parseSessionEntries(content);
		manager.migrateSessionEntries(entries);
		return entries.map((entry) => JSON.stringify(entry));
	} finally {
		crypto.randomUUID = original;
		syncBuiltinESMExports();
	}
}

function handle(request) {
	switch (request.operation) {
		case "migrate_collision":
			return migrateWithCollisions(request.content);
		case "parse":
			return manager
				.parseSessionEntries(request.content)
				.map((entry) => JSON.stringify(entry));
		case "migrate": {
			const entries = manager.parseSessionEntries(request.content);
			manager.migrateSessionEntries(entries);
			return entries.map((entry) => JSON.stringify(entry));
		}
		case "serialize":
			return manager
				.parseSessionEntries(request.content)
				.map(output.serializeSessionEntryForPersistence);
		case "write":
			writeFileSync(request.path, request.content);
			return readFileSync(request.path, "utf8");
		case "read":
			return snapshot(
				manager.SessionManager.open(request.path, request.sessionDir),
			);
		case "update_summary": {
			const session = manager.SessionManager.open(
				request.path,
				request.sessionDir,
			);
			session.updateCompactionSummary(request.entryId, request.summary);
			return session.getEntries().map((entry) => JSON.stringify(entry));
		}
		default:
			throw new Error(`Unknown record oracle operation: ${request.operation}`);
	}
}

try {
	for await (const line of createInterface({ input: process.stdin })) {
		if (!line.trim()) continue;
		const request = JSON.parse(line);
		try {
			process.stdout.write(
				`${JSON.stringify({ id: request.id, ok: true, output: handle(request) })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ id: request.id, ok: false, error: String(error) })}\n`,
			);
		}
	}
} finally {
	rmSync(sandbox, { recursive: true, force: true });
}
