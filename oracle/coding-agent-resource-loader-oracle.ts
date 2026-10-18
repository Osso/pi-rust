import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ResourceLoaderApi = {
	loadRulesFilesFromDir(dirPath: string): Array<{ path: string; content: string }>;
	loadRulesFromDir(dirPath: string): string | undefined;
	loadProjectContextFiles(options: {
		cwd: string;
		agentDir: string;
	}): Array<{ path: string; content: string }>;
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

function executeRequestLine(line: string, api: ResourceLoaderApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as {
			id: RequestId;
			operation: "rules" | "context_files";
			dir: string;
			cwd: string;
			agentDir: string;
		};
		id = request.id;
		let output: unknown;
		if (request.operation === "rules") {
			output = {
				files: api.loadRulesFilesFromDir(request.dir),
				content: api.loadRulesFromDir(request.dir) ?? null,
			};
		} else if (request.operation === "context_files") {
			output = { files: api.loadProjectContextFiles({ cwd: request.cwd, agentDir: request.agentDir }) };
		} else {
			throw new Error(`unknown operation: ${String(request.operation)}`);
		}
		const response: Response = { id, ok: true, output };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: ResourceLoaderApi): void {
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
		throw new Error("resource loader oracle requires a source module path");
	}
	const api = (await import(pathToFileURL(sourceModulePath).href)) as ResourceLoaderApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`resource loader oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
