import { selectApprovedModule } from "./approved-delta.mjs";
import { StringDecoder } from "node:string_decoder";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type SchemaNode = {
	t: string;
	options?: Record<string, unknown>;
	properties?: Record<string, SchemaNode>;
	schema?: SchemaNode | Record<string, unknown>;
	anyOf?: SchemaNode[];
	oneOf?: SchemaNode[];
	allOf?: SchemaNode[];
	items?: SchemaNode | SchemaNode[];
	key?: SchemaNode;
	value?: SchemaNode;
	values?: unknown[];
	valueLiteral?: unknown;
	ref?: string;
	defs?: Record<string, SchemaNode>;
};

type ToolSpec = {
	name: string;
	kind: "typebox" | "plain";
	schema: SchemaNode | Record<string, unknown>;
};

type Case = {
	name: string;
	kind: "typebox" | "plain";
	schema: SchemaNode | Record<string, unknown>;
	arguments: Record<string, unknown>;
	toolName?: string;
	tools?: ToolSpec[];
};

type Request = {
	id: RequestId;
	operation: "validate_tool_arguments" | "validate_tool_call";
	cases: Case[];
};

type ValidationApi = {
	validateToolArguments(tool: unknown, toolCall: unknown): unknown;
	validateToolCall(tools: unknown[], toolCall: unknown): unknown;
};

type TypeboxApi = {
	Type: {
		Object(properties: Record<string, unknown>, options?: Record<string, unknown>): unknown;
		Optional(schema: unknown): unknown;
		String(options?: Record<string, unknown>): unknown;
		Number(options?: Record<string, unknown>): unknown;
		Integer(options?: Record<string, unknown>): unknown;
		Boolean(options?: Record<string, unknown>): unknown;
		Null(options?: Record<string, unknown>): unknown;
		Any(options?: Record<string, unknown>): unknown;
		Unknown(options?: Record<string, unknown>): unknown;
		Literal(value: unknown, options?: Record<string, unknown>): unknown;
		Union(schemas: unknown[], options?: Record<string, unknown>): unknown;
		Array(items: unknown, options?: Record<string, unknown>): unknown;
		Tuple(items: unknown[], options?: Record<string, unknown>): unknown;
		Record(key: unknown, value: unknown, options?: Record<string, unknown>): unknown;
		Enum(values: unknown, options?: Record<string, unknown>): unknown;
		Unsafe(schema: Record<string, unknown>): unknown;
		Intersect(schemas: unknown[], options?: Record<string, unknown>): unknown;
		Ref(ref: string, options?: Record<string, unknown>): unknown;
		Cyclic(defs: Record<string, unknown>, ref: string, options?: Record<string, unknown>): unknown;
	};
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function buildSchema(typebox: TypeboxApi["Type"], node: SchemaNode): unknown {
	const options = node.options ?? {};
	switch (node.t) {
		case "object": {
			const properties: Record<string, unknown> = {};
			for (const [key, value] of Object.entries(node.properties ?? {})) {
				properties[key] = buildSchema(typebox, value);
			}
			return typebox.Object(properties, options);
		}
		case "optional":
			return typebox.Optional(buildSchema(typebox, node.schema as SchemaNode));
		case "string":
			return typebox.String(options);
		case "number":
			return typebox.Number(options);
		case "integer":
			return typebox.Integer(options);
		case "boolean":
			return typebox.Boolean(options);
		case "null":
			return typebox.Null(options);
		case "any":
			return typebox.Any(options);
		case "unknown":
			return typebox.Unknown(options);
		case "literal":
			return typebox.Literal(node.valueLiteral, options);
		case "union":
			return typebox.Union((node.anyOf ?? []).map((item) => buildSchema(typebox, item)), options);
		case "array":
			return typebox.Array(buildSchema(typebox, node.items as SchemaNode), options);
		case "tuple":
			return typebox.Tuple((node.items as SchemaNode[]).map((item) => buildSchema(typebox, item)), options);
		case "record":
			return typebox.Record(
				buildSchema(typebox, node.key as SchemaNode),
				buildSchema(typebox, node.value as SchemaNode),
				options,
			);
		case "enum":
			return typebox.Enum(node.values, options);
		case "unsafe":
			return typebox.Unsafe(node.schema as Record<string, unknown>);
		case "intersect":
			return typebox.Intersect((node.allOf ?? []).map((item) => buildSchema(typebox, item)), options);
		case "ref":
			return typebox.Ref(node.ref as string, options);
		case "cyclic": {
			const defs: Record<string, unknown> = {};
			for (const [key, value] of Object.entries(node.defs ?? {})) {
				defs[key] = buildSchema(typebox, value);
			}
			return typebox.Cyclic(defs, node.ref as string, options);
		}
		default:
			throw new Error(`unsupported schema node ${node.t}`);
	}
}

function materialize(typebox: TypeboxApi["Type"], kind: string, schema: unknown): unknown {
	return kind === "typebox" ? buildSchema(typebox, schema as SchemaNode) : schema;
}

function createTool(typebox: TypeboxApi["Type"], spec: { name: string; kind: string; schema: unknown }): {
	name: string;
	description: string;
	parameters: unknown;
} {
	return {
		name: spec.name,
		description: "oracle tool",
		parameters: materialize(typebox, spec.kind, spec.schema),
	};
}

function createToolCall(name: string, args: Record<string, unknown>): unknown {
	return { type: "toolCall", id: "tool-1", name, arguments: args };
}

function execute(request: Request, api: ValidationApi, typebox: TypeboxApi["Type"]): unknown {
	const results = request.cases.map((testCase) => {
		let schemas: unknown[] = [];
		try {
			const tools =
				request.operation === "validate_tool_call"
					? (testCase.tools ?? []).map((spec) => createTool(typebox, spec))
					: [
							createTool(typebox, {
								name: testCase.toolName ?? "echo",
								kind: testCase.kind,
								schema: testCase.schema,
							}),
						];
			schemas = tools.map((tool) => tool.parameters);
			const toolCall = createToolCall(testCase.toolName ?? "echo", testCase.arguments);
			const value =
				request.operation === "validate_tool_call"
					? api.validateToolCall(tools, toolCall)
					: api.validateToolArguments(tools[0], toolCall);
			return { name: testCase.name, ok: true, value, schemas, argumentsAfter: testCase.arguments };
		} catch (error) {
			return { name: testCase.name, ok: false, error: normalizeError(error), schemas, argumentsAfter: testCase.arguments };
		}
	});
	return { id: request.id, ok: true, output: { results } };
}

function executeRequestLine(line: string, api: ValidationApi, typebox: TypeboxApi["Type"]): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, api, typebox))}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(api: ValidationApi, typebox: TypeboxApi["Type"]): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api, typebox);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("validation oracle requires a source module path");
	}
	const module = selectApprovedModule(sourceModulePath, "packages/ai/src/utils/validation.ts");
	const api = (await import(pathToFileURL(module).href)) as ValidationApi;
	const require = createRequire(sourceModulePath);
	const typebox = (await import(pathToFileURL(require.resolve("typebox")).href)) as TypeboxApi;
	serveRequests(api, typebox);
}

void main().catch((error: unknown) => {
	process.stderr.write(`validation oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
