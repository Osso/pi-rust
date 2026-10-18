import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type SessionManagerApi = {
	SessionManager: {
		open(path: string, sessionDir?: string, cwdOverride?: string): any;
		create(cwd: string, sessionDir?: string, options?: unknown): any;
		list(cwd: string, sessionDir?: string, onProgress?: unknown, controlDbPath?: string): Promise<any[]>;
		listAll(sessionDir?: string, onProgress?: unknown, controlDbPath?: string): Promise<any[]>;
	};
	parseSessionEntries(content: string): unknown[];
	buildContextEntries(entries: unknown[], leafId?: string | null): unknown[];
	buildSessionContext(entries: unknown[], leafId?: string | null): unknown;
	loadEntriesFromFile(path: string): unknown[];
	readSessionHeader(path: string): unknown;
	findMostRecentSession(dir: string, cwd?: string): string | null;
	getDefaultSessionDir(cwd: string, agentDir?: string): string;
	migrateSessionEntries(entries: unknown[]): void;
};

type SessionToolOutputApi = {
	serializeSessionEntryForPersistence(entry: unknown): string;
	truncateToolResultForPersistence(entry: unknown): unknown;
};

type OracleApi = SessionManagerApi & SessionToolOutputApi;

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

async function loadApi(modulePath: string): Promise<OracleApi> {
	const managerUrl = pathToFileURL(modulePath).href;
	const toolOutputUrl = pathToFileURL(join(dirname(modulePath), "session-tool-output.ts")).href;
	const manager = (await import(managerUrl)) as SessionManagerApi;
	const toolOutput = (await import(toolOutputUrl)) as SessionToolOutputApi;
	return { ...manager, ...toolOutput };
}

function readSession(api: OracleApi, request: any): unknown {
	const manager = api.SessionManager.open(request.path, request.sessionDir);
	const entries = manager.getEntries();
	const context = manager.buildSessionContext();
	return {
		id: manager.getSessionId(),
		cwd: manager.getCwd(),
		leafId: manager.getLeafId(),
		entries,
		entryLines: entries.map((entry: unknown) => JSON.stringify(entry)),
		context,
		contextMessageLines: context.messages.map((message: unknown) => JSON.stringify(message)),
		tree: manager.getTree(),
		header: manager.getHeader(),
	};
}

async function handle(api: OracleApi, request: any): Promise<unknown> {
	switch (request.operation) {
		case "serialize_entries":
			return {
				lines: request.entries.map((entry: unknown) => api.serializeSessionEntryForPersistence(entry)),
			};
		case "parse_entries": {
			const entries = api.parseSessionEntries(request.content);
			return { entries, entryLines: entries.map((entry) => JSON.stringify(entry)) };
		}
		case "migrate_entries": {
			const entries = structuredClone(request.entries);
			api.migrateSessionEntries(entries);
			return { entries };
		}
		case "build_context": {
			const leafId = "leafId" in request ? request.leafId : undefined;
			const context = api.buildSessionContext(request.entries, leafId);
			return {
				contextEntries: api.buildContextEntries(request.entries, leafId).map((entry: any) => entry.id),
				context,
				contextMessageLines: context.messages.map((message: unknown) => JSON.stringify(message)),
			};
		}
		case "load_entries": {
			const entries = api.loadEntriesFromFile(request.path);
			return { entries, entryLines: entries.map((entry: unknown) => JSON.stringify(entry)) };
		}
		case "read_header": {
			const header = api.readSessionHeader(request.path);
			return { header, headerJson: header === null ? null : JSON.stringify(header) };
		}
		case "find_most_recent":
			return { path: api.findMostRecentSession(request.dir, request.cwd) };
		case "default_session_dir":
			return { path: api.getDefaultSessionDir(request.cwd, request.agentDir) };
		case "read_session":
			return readSession(api, request);
		case "write_session": {
			const manager = api.SessionManager.create(request.cwd, request.dir, { id: request.id });
			for (const message of request.messages) {
				manager.appendMessage(message);
			}
			const entries = manager.getEntries();
			return {
				path: manager.getSessionFile(),
				entries,
				entryLines: entries.map((entry: unknown) => JSON.stringify(entry)),
			};
		}
		case "append_message": {
			const manager = api.SessionManager.open(request.path, request.sessionDir);
			manager.appendMessage(request.message);
			const entries = manager.getEntries();
			const context = manager.buildSessionContext();
			return {
				entries,
				entryLines: entries.map((entry: unknown) => JSON.stringify(entry)),
				context,
				contextMessageLines: context.messages.map((message: unknown) => JSON.stringify(message)),
			};
		}
		case "truncate_tool_result": {
			const entry = api.truncateToolResultForPersistence(request.entry);
			return { entry, entryJson: JSON.stringify(entry) };
		}
		default:
			throw new Error(`unknown operation: ${request.operation}`);
	}
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("session-manager oracle requires the source module path");
	const api = await loadApi(modulePath);

	const decoder = new TextDecoder();
	let buffer = "";
	for await (const chunk of process.stdin) {
		buffer += decoder.decode(chunk, { stream: true });
		let newline = buffer.indexOf("\n");
		while (newline !== -1) {
			const line = buffer.slice(0, newline).trim();
			buffer = buffer.slice(newline + 1);
			if (line.length > 0) {
				const request = JSON.parse(line);
				let response: Response;
				try {
					response = { id: request.id ?? null, ok: true, output: await handle(api, request) };
				} catch (error) {
					response = { id: request.id ?? null, ok: false, error: normalizeError(error) };
				}
				process.stdout.write(`${JSON.stringify(response)}\n`);
			}
			newline = buffer.indexOf("\n");
		}
	}
}

void main();
