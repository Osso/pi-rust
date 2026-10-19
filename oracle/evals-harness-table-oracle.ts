import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Harness = {
	name: string;
	run(input: unknown, context: { artifacts: Record<string, unknown>; setArtifact(name: string, value: unknown): void }): Promise<unknown>;
};

type HarnessTableApi = {
	deriveEvalGroupKey(input: unknown, repetition: number): string;
	parseEvalHarnessIterationArtifact(value: unknown): unknown;
	evalHarnessTable(
		evalSet: string,
		options: {
			baseline: Harness;
			candidate?: Harness;
			candidates?: Harness[];
			repetitions?: number;
		},
	): Array<{ harness: Harness; name: string; repetition: number }>;
};

type HarnessTableRequest =
	| { id: RequestId; operation: "group_key"; input: unknown; repetition: number }
	| { id: RequestId; operation: "parse_artifact"; value?: unknown }
	| {
			id: RequestId;
			operation: "table_plan";
			evalSet: string;
			baseline: string;
			candidates: string[];
			repetitions?: number;
			input: unknown;
	  };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function fakeHarness(name: string): Harness {
	return {
		name,
		async run(input) {
			return {
				output: { harness: name, input },
				session: { events: [{ type: "message", role: "user", content: name }] },
				usage: {},
				errors: [],
				artifacts: {},
			};
		},
	};
}

async function execute(request: HarnessTableRequest, api: HarnessTableApi): Promise<Response> {
	try {
		if (request.operation === "group_key") {
			return { id: request.id, ok: true, output: { value: api.deriveEvalGroupKey(request.input, request.repetition) } };
		}
		if (request.operation === "parse_artifact") {
			const parsed = api.parseEvalHarnessIterationArtifact(request.value);
			return { id: request.id, ok: true, output: { parsed: parsed ?? null } };
		}
		const rows = api.evalHarnessTable(request.evalSet, {
			baseline: fakeHarness(request.baseline),
			candidates: request.candidates.map((name) => fakeHarness(name)),
			...(request.repetitions === undefined ? {} : { repetitions: request.repetitions }),
		});
		const executed = [];
		for (const row of rows) {
			const context = {
				artifacts: {} as Record<string, unknown>,
				setArtifact(name: string, value: unknown) {
					context.artifacts[name] = value;
				},
			};
			const run = (await row.harness.run(request.input, context)) as {
				output: unknown;
				artifacts?: Record<string, unknown>;
			};
			executed.push({
				name: row.name,
				repetition: row.repetition,
				output: run.output,
				artifacts: run.artifacts ?? null,
			});
		}
		return { id: request.id, ok: true, output: { rows: executed } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

async function executeRequestLine(line: string, api: HarnessTableApi): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as HarnessTableRequest;
		id = request.id;
		process.stdout.write(`${JSON.stringify(await execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: HarnessTableApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let chain = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			chain = chain.then(() => executeRequestLine(line, api));
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) throw new Error("evals harness-table oracle requires a source module path");
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as HarnessTableApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`evals harness-table oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
