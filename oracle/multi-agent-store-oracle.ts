import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type StoreStep = {
	op: string;
	args: unknown[];
};

type StoreRequest = {
	id: RequestId;
	now?: string;
	steps: StoreStep[];
};

type StoreModule = {
	MultiAgentStore: new (options?: { now?: () => string }) => StoreLike;
};

type StoreLike = Record<string, (...args: unknown[]) => unknown>;

type StepResult = { ok: true; value: unknown } | { ok: false; error: { name: string; message: string } };

type Response =
	| { id: RequestId; ok: true; output: StepResult[] }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

const LIFECYCLE_VALUES = new Set(["completed", "failed", "waiting_for_input"]);

function executeStep(store: StoreLike, step: StoreStep): StepResult {
	try {
		const method = store[step.op];
		if (typeof method !== "function") {
			throw new Error(`unsupported store operation ${step.op}`);
		}
		const args = step.args.map((argument, index) => {
			if (step.op === "listPendingLifecycleNotificationsForAgent" && index === 1) {
				if (typeof argument !== "string" || !LIFECYCLE_VALUES.has(argument)) {
					throw new Error(`invalid lifecycle ${String(argument)}`);
				}
				return argument;
			}
			return argument;
		});
		const value = method.apply(store, args);
		return { ok: true, value: value === undefined ? null : value };
	} catch (error) {
		return { ok: false, error: normalizeError(error) };
	}
}

function execute(request: StoreRequest, module: StoreModule): Response {
	try {
		const store = new module.MultiAgentStore(
			request.now === undefined ? {} : { now: () => request.now as string },
		);
		const output = request.steps.map((step) => executeStep(store, step));
		return { id: request.id, ok: true, output };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, module: StoreModule): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as StoreRequest;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, module))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(module: StoreModule): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), module);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("multi-agent store oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const module = (await import(pathToFileURL(readSourceModulePath()).href)) as StoreModule;
	serveRequests(module);
}

void main().catch((error: unknown) => {
	process.stderr.write(`multi-agent store oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
