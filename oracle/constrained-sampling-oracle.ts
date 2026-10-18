import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request =
	| { id: number; operation: "strict"; schema: unknown }
	| { id: number; operation: "resolveJson"; tool: unknown; supportsStrictMode: boolean }
	| { id: number; operation: "resolveGrammar"; tool: unknown; supports: boolean }
	| { id: number; operation: "getInput"; toolName: string; arguments: Record<string, unknown>; inputProperty: string }
	| { id: number; operation: "append"; inputProperty: string; steps: Array<{ nextInput: string; close: boolean }> }
	| { id: number; operation: "properties"; tools: unknown[] | null; supports: boolean };

type SamplingApi = {
	makeStrictJsonSchema(schema: unknown): unknown;
	resolveJsonSchemaStrictSampling(tool: unknown, supportsStrictMode: boolean): boolean | undefined;
	resolveGrammarConstrainedSampling(
		tool: unknown,
		supportsOpenAIGrammarTools: boolean,
	): { format: string; definition: string; inputProperty: string } | undefined;
	getGrammarToolInput(toolName: string, arguments_: Record<string, unknown>, inputProperty: string): string;
	appendGrammarToolInputJsonDelta(
		buffer: { input: string; started: boolean; closed: boolean },
		inputProperty: string,
		nextInput: string,
		close: boolean,
	): string | undefined;
	createGrammarToolInputProperties(tools: unknown[] | undefined, supportsOpenAIGrammarTools: boolean): Map<string, string>;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: SamplingApi) {
	try {
		switch (request.operation) {
			case "strict":
				return { id: request.id, ok: true, output: { schema: api.makeStrictJsonSchema(request.schema) } };
			case "resolveJson":
				return {
					id: request.id,
					ok: true,
					output: { strict: api.resolveJsonSchemaStrictSampling(request.tool, request.supportsStrictMode) ?? null },
				};
			case "resolveGrammar":
				return {
					id: request.id,
					ok: true,
					output: { grammar: api.resolveGrammarConstrainedSampling(request.tool, request.supports) ?? null },
				};
			case "getInput":
				return {
					id: request.id,
					ok: true,
					output: { input: api.getGrammarToolInput(request.toolName, request.arguments, request.inputProperty) },
				};
			case "append": {
				const buffer = { input: "", started: false, closed: false };
				const deltas = request.steps.map((step) =>
					api.appendGrammarToolInputJsonDelta(buffer, request.inputProperty, step.nextInput, step.close) ?? null,
				);
				return { id: request.id, ok: true, output: { deltas, buffer } };
			}
			case "properties": {
				const properties = api.createGrammarToolInputProperties(request.tools ?? undefined, request.supports);
				return { id: request.id, ok: true, output: { properties: [...properties.entries()] } };
			}
		}
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: SamplingApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			process.stdout.write(`${JSON.stringify(execute(JSON.parse(buffer.slice(0, lineFeed)), api))}\n`);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("constrained sampling oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as SamplingApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`constrained sampling oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
