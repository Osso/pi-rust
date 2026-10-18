import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type CliApi = {
	handlePackageCommand(args: string[]): Promise<boolean>;
	handleConfigCommand(args: string[]): Promise<boolean>;
};

type Request = {
	id: RequestId;
	operation: string;
	args: string[];
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

async function execute(request: Request, api: CliApi): Promise<unknown> {
	const stdout: string[] = [];
	const stderr: string[] = [];
	const originalLog = console.log;
	const originalError = console.error;
	console.log = (...args: unknown[]) => {
		stdout.push(args.map((arg) => String(arg)).join(" "));
	};
	console.error = (...args: unknown[]) => {
		stderr.push(args.map((arg) => String(arg)).join(" "));
	};
	const originalExitCode = process.exitCode;
	process.exitCode = undefined;
	try {
		const handled =
			request.operation === "handle_config"
				? await api.handleConfigCommand(request.args)
				: await api.handlePackageCommand(request.args);
		return { handled, stdout, stderr, exitCode: process.exitCode ?? null };
	} finally {
		console.log = originalLog;
		console.error = originalError;
		process.exitCode = originalExitCode;
	}
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("package manager cli oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as unknown as CliApi;
	const decoder = new TextDecoder("utf-8");
	let buffer = "";
	let queue = Promise.resolve();
	const drain = () => {
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(async () => {
				let id: RequestId = null;
				try {
					const request = JSON.parse(line) as Request;
					id = request.id;
					process.stdout.write(`${JSON.stringify({ id, ok: true, output: await execute(request, api) })}\n`);
				} catch (error) {
					process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
				}
			});
		}
	};
	process.stdin.on("data", (chunk: Buffer) => {
		buffer += decoder.decode(chunk, { stream: true });
		drain();
	});
	void queue;
}

void main().catch((error: unknown) => {
	process.stderr.write(`package manager cli oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
