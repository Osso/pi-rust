import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type BuiltinSlashCommand = {
	name: string;
	description: string;
	argumentHint?: string;
};

type SlashCommandsApi = {
	BUILTIN_SLASH_COMMANDS: ReadonlyArray<BuiltinSlashCommand>;
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

function executeRequestLine(line: string, api: SlashCommandsApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as { id: RequestId };
		id = request.id;
		const commands = api.BUILTIN_SLASH_COMMANDS.map((command) => ({
			name: command.name,
			description: command.description,
			argumentHint: command.argumentHint ?? null,
		}));
		const response: Response = { id, ok: true, output: { commands } };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: SlashCommandsApi): void {
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
		throw new Error("slash commands oracle requires a source module path");
	}
	const api = (await import(pathToFileURL(sourceModulePath).href)) as SlashCommandsApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`slash commands oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
