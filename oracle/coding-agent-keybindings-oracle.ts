import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type KeybindingsApi = {
	KeybindingsManager: {
		create(agentDir?: string): {
			getUserBindings(): Record<string, string | string[]>;
			getEffectiveConfig(): Record<string, string | string[]>;
			getKeys(keybinding: string): string[];
		};
	};
	migrateKeybindingsConfig(rawConfig: Record<string, unknown>): {
		config: Record<string, unknown>;
		migrated: boolean;
	};
};

type Request =
	| { id: RequestId; operation: "migrate"; config: Record<string, unknown> }
	| { id: RequestId; operation: "load"; agentDir: string; keys: string[] };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function entries(value: Record<string, string | string[]>): Array<[string, string[]]> {
	return Object.entries(value).map(([key, keys]) => [key, Array.isArray(keys) ? keys : [keys]]);
}

function execute(request: Request, api: KeybindingsApi): unknown {
	if (request.operation === "migrate") {
		const result = api.migrateKeybindingsConfig(request.config);
		return { config: Object.entries(result.config), migrated: result.migrated };
	}
	const manager = api.KeybindingsManager.create(request.agentDir);
	return {
		effective: entries(manager.getEffectiveConfig()),
		keys: request.keys.map((keybinding) => manager.getKeys(keybinding)),
	};
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
	const api = (await import(pathToFileURL(sourceModulePath).href)) as KeybindingsApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`keybindings oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
