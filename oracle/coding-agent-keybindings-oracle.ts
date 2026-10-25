import { rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Manager = {
	getUserBindings(): Record<string, string | string[]>;
	getEffectiveConfig(): Record<string, string | string[]>;
	getKeys(keybinding: string): string[];
	getConflicts(): unknown[];
	matches(data: string, keybinding: string): boolean;
	reload(): void;
};

type KeybindingsApi = {
	KEYBINDINGS: Record<
		string,
		{ defaultKeys: string | string[]; description: string }
	>;
	KeybindingsManager: {
		new (userBindings?: Record<string, string | string[]>): Manager;
		create(agentDir?: string): Manager;
	};
	migrateKeybindingsConfig(rawConfig: Record<string, unknown>): {
		config: Record<string, unknown>;
		migrated: boolean;
	};
};

type Request =
	| { id: RequestId; operation: "migrate"; config: Record<string, unknown> }
	| { id: RequestId; operation: "load"; agentDir: string }
	| { id: RequestId; operation: "definitions" }
	| {
			id: RequestId;
			operation: "reload";
			agentDir: string;
			contents: Array<string | null>;
			withoutPath: boolean;
	  };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: KeybindingsApi): unknown {
	if (request.operation === "definitions") {
		return Object.entries(api.KEYBINDINGS).map(([id, definition]) => ({
			id,
			keys: Array.isArray(definition.defaultKeys)
				? definition.defaultKeys
				: [definition.defaultKeys],
			description: definition.description,
		}));
	}
	if (request.operation === "reload") return captureReloads(request, api);
	if (request.operation === "migrate") {
		const result = api.migrateKeybindingsConfig(request.config);
		return { config: Object.entries(result.config), migrated: result.migrated };
	}
	const manager = api.KeybindingsManager.create(request.agentDir);
	const ids = Object.keys(manager.getEffectiveConfig());
	return {
		ids,
		keys: ids.map((keybinding) => manager.getKeys(keybinding)),
	};
}

function captureReloads(
	request: Extract<Request, { operation: "reload" }>,
	api: KeybindingsApi,
): unknown {
	const path = join(request.agentDir, "keybindings.json");
	const manager = request.withoutPath
		? new api.KeybindingsManager({ "app.tools.expand": "ctrl+w" })
		: api.KeybindingsManager.create(request.agentDir);
	return request.contents.map((content) => {
		if (content === null) rmSync(path, { force: true });
		else writeFileSync(path, content);
		manager.reload();
		const entries = (config: Record<string, string | string[]>) =>
			Object.entries(config).map(([id, keys]) => [
				id,
				Array.isArray(keys) ? keys : [keys],
			]);
		return {
			user: entries(manager.getUserBindings()),
			effective: entries(manager.getEffectiveConfig()),
			conflicts: manager.getConflicts(),
			matches: [
				manager.matches("\u0017", "app.tools.expand"),
				manager.matches("\u0018", "app.tools.expand"),
				manager.matches("\u0018", "app.interrupt"),
			],
		};
	});
}

function executeRequestLine(line: string, api: KeybindingsApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const response: Response = { id, ok: true, output: execute(request, api) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: KeybindingsApi): void {
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8");
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("keybindings oracle requires a source module path");
	}
	const api = (await import(
		pathToFileURL(sourceModulePath).href
	)) as KeybindingsApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`keybindings oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
