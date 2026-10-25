import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import type * as Source from "@pi-oracle-source/coding-agent/src/core/session-manager.ts";

type Api = typeof Source;
type Manager = Source.SessionManager;
const api = (await import(pathToFileURL(process.argv[2]).href)) as Api;
const control = (await import(
	new URL("./session-control-db.ts", pathToFileURL(process.argv[2])).href
)) as typeof import("@pi-oracle-source/coding-agent/src/core/session-control-db.ts");

type Request = {
	id: number;
	mode:
		| "open"
		| "memory"
		| "create"
		| "fork"
		| "load"
		| "recent"
		| "continue"
		| "list"
		| "listAll"
		| "info"
		| "default";
	path: string;
	cwd: string;
	dir?: string;
	sessionId?: string;
	from?: string | null;
	commands?: Command[];
};
type Command = { op: string; text?: string; target?: string; path?: string };

function requireSessionPath(manager: Manager): string {
	const path = manager.getSessionFile();
	if (!path) throw new Error("command requires a session file");
	return path;
}

function readSnapshotBytes(path: string | undefined): string | null {
	if (!path) return null;
	try {
		return readFileSync(path, "utf8");
	} catch (error) {
		// Missing/unflushed files and deliberate directory replacement have no bytes.
		if (
			error instanceof Error &&
			"code" in error &&
			["ENOENT", "EISDIR"].includes(String(error.code))
		)
			return null;
		throw error;
	}
}

function snapshot(manager: Manager) {
	const path = manager.getSessionFile();
	return {
		path,
		header: manager.getHeader(),
		entries: manager.getEntries(),
		tree: manager.getTree(),
		context: manager.buildSessionContext(),
		contextEntries: manager.buildContextEntries(),
		cwd: manager.getCwd(),
		leaf: manager.getLeafId(),
		name: manager.getSessionName() ?? null,
		nameState: manager.hasSessionNameState(),
		persisted: manager.isPersisted(),
		bytes: readSnapshotBytes(path),
	};
}

function appendAssistant(manager: Manager, text: string): string {
	return manager.appendMessage({
		role: "assistant",
		content: [{ type: "text", text }],
		api: "anthropic-messages",
		provider: "anthropic",
		model: "fixture",
		usage: {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		},
		stopReason: "stop",
		timestamp: 2,
	});
}

function persistLegacyFixture(manager: Manager, version: number): void {
	const header = manager.getHeader();
	if (!header) throw new Error("missing legacy header");
	header.version = version;
	if (version === 1) {
		for (const entry of manager.getEntries()) {
			const raw = entry as unknown as Record<string, unknown>;
			delete raw.id;
			delete raw.parentId;
		}
	}
	manager.persistForRecovery();
}

function writeArchivedGoalFixture(
	manager: Manager,
	goal: string,
): string | undefined {
	const db = manager.getMetadataControlDbPath();
	const path = requireSessionPath(manager);
	if (!db) throw new Error("archiveGoal requires an attached database");
	const metadata = control.readSessionMetadata(db, path);
	if (!metadata) throw new Error("missing session metadata");
	const archive = `${path}.zst`;
	control.writeSessionMetadata(db, { ...metadata, sessionPath: archive });
	control.writeSessionGoal(db, archive, goal);
	writeFileSync(archive, "archive path probe");
	rmSync(path);
	return manager.getSessionGoalJsonForSession(path);
}

function apply(manager: Manager, command: Command): unknown {
	const text = command.text ?? "";
	const target = command.target ?? "";
	switch (command.op) {
		case "user":
			return manager.appendMessage({
				role: "user",
				content: text,
				timestamp: 1,
			});
		case "assistant":
			return appendAssistant(manager, text);
		case "custom":
			return manager.appendCustomEntry("state", { text, nested: { n: 2 } });
		case "thinking":
			return manager.appendThinkingLevelChange(text);
		case "model":
			return manager.appendModelChange("anthropic", text);
		case "label":
			return manager.appendLabelChange(target, command.text);
		case "branch":
			return manager.branch(target);
		case "summary":
			return manager.branchWithSummary(
				command.target ?? null,
				text,
				{ source: "repin" },
				false,
			);
		case "reset":
			return manager.resetLeaf();
		case "legacy":
			return persistLegacyFixture(manager, Number(text));
		case "persist":
			return manager.persistForRecovery();
		case "relocate":
			return manager.relocate(text, command.path);
		case "sandbox":
			return manager.setSessionSandboxProfile(undefined);
		case "db":
			return manager.setMetadataControlDbPath(command.path);
		case "archiveGoal":
			return writeArchivedGoalFixture(manager, text);
		case "goal":
			return manager.getSessionGoalJsonForSession(command.path ?? "");
		case "setGoal":
			return manager.setSessionGoalJson(text);
		case "name":
			return manager.setSessionName(command.text);
		case "switch":
			return manager.setSessionFile(command.path ?? "");
		case "compaction":
			return manager.appendCompaction(
				text,
				target,
				123,
				{ source: "repin" },
				false,
				42,
			);
		case "update":
			return manager.updateCompactionSummary(target, text);
		case "extract":
			return manager.createBranchedSession(target);
		case "remove":
			return rmSync(requireSessionPath(manager), { force: true });
		case "directory": {
			const path = requireSessionPath(manager);
			rmSync(path, { force: true });
			mkdirSync(path);
			return;
		}
		default:
			throw new Error(`Unknown command ${command.op}`);
	}
}

async function readListing(request: Request) {
	const progress: number[][] = [];
	const onProgress = (loaded: number, total: number) => {
		progress.push([loaded, total]);
	};
	const sessions =
		request.mode === "list"
			? await api.SessionManager.list(request.cwd, request.dir, onProgress)
			: await api.SessionManager.listAll(request.dir, onProgress);
	return { sessions, progress };
}

function createManager(request: Request): Manager {
	const options =
		request.sessionId === undefined ? undefined : { id: request.sessionId };
	switch (request.mode) {
		case "memory":
			return api.SessionManager.inMemory(request.cwd, options);
		case "create":
			return api.SessionManager.create(request.cwd, request.dir, options);
		case "fork":
			return api.SessionManager.forkFrom(
				request.path,
				request.cwd,
				request.dir,
				options,
				request.from,
			);
		case "continue":
			return api.SessionManager.continueRecent(request.cwd, request.dir);
		default:
			return api.SessionManager.open(request.path, request.dir);
	}
}

function executeCommands(manager: Manager, commands: Command[]): unknown[] {
	const results: unknown[] = [];
	for (const command of commands) {
		const target = command.target?.startsWith("$")
			? results[Number(command.target.slice(1))]
			: command.target;
		results.push(
			apply(manager, {
				...command,
				target: typeof target === "string" ? target : undefined,
			}) ?? null,
		);
	}
	return results;
}

async function readWithoutManager(request: Request): Promise<unknown> {
	switch (request.mode) {
		case "load":
			return api.loadEntriesFromFile(request.path);
		case "recent": {
			if (request.dir === undefined)
				throw new Error("recent requires a directory");
			return api.findMostRecentSession(request.dir, request.cwd);
		}
		case "info":
			return api.buildSessionInfo(request.path);
		case "default":
			return api.getDefaultSessionDir(request.cwd, request.dir);
		default:
			return readListing(request);
	}
}

function writeResponse(response: unknown): void {
	process.stdout.write(`${JSON.stringify(response)}\n`);
}

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as Request;
	let manager: Manager | undefined;
	try {
		if (
			["load", "recent", "list", "listAll", "info", "default"].includes(
				request.mode,
			)
		) {
			writeResponse({
				id: request.id,
				ok: true,
				output: await readWithoutManager(request),
			});
			continue;
		}
		manager = createManager(request);
		const results = executeCommands(manager, request.commands ?? []);
		writeResponse({
			id: request.id,
			ok: true,
			output: { results, state: snapshot(manager) },
		});
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		const fields = Object.fromEntries(
			Object.entries(error).filter(([key]) => key !== "stack"),
		);
		writeResponse({
			id: request.id,
			ok: false,
			error: { name: error.name, message: error.message, ...fields },
			state: manager ? snapshot(manager) : null,
		});
	}
}
