import { readFileSync } from "node:fs";
import { join } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

const lock = JSON.parse(
	readFileSync(
		new URL("../docs/porting/source-lock.json", import.meta.url),
		"utf8",
	),
);
const SOURCE_ROOT = join(lock.worktree, "packages/agent-core/src/harness");

type ToolName = "read" | "write" | "edit" | "bash";

type Request =
	| { id: number; operation: "schemas" }
	| {
			id: number;
			operation: "tool";
			root: string;
			tool: ToolName;
			input: unknown;
			abort: boolean;
			updates: boolean;
			imageProcessor: boolean;
			prepare: boolean;
	  };

type Response =
	| { id: number; ok: true; result: unknown }
	| { id: number; ok: false; error: { message: string } };

type Tool = {
	name: string;
	label: string;
	description: string;
	parameters: unknown;
	execute(
		toolCallId: string,
		params: unknown,
		signal: AbortSignal | undefined,
		onUpdate: ((update: unknown) => void) | undefined,
		context: { env: unknown },
	): Promise<{ content: unknown[]; details?: unknown }>;
};

type ToolFactory = (options?: unknown) => Tool;

type Apis = {
	NodeExecutionEnv: new (options: { cwd: string }) => unknown;
	createReadTool: ToolFactory;
	createWriteTool: ToolFactory;
	createEditTool: ToolFactory;
	createBashTool: ToolFactory;
};

async function loadApis(): Promise<Apis> {
	const env = (await import(
		pathToFileURL(`${SOURCE_ROOT}/env/nodejs.ts`).href
	)) as {
		NodeExecutionEnv: new (options: { cwd: string }) => unknown;
	};
	const bash = (await import(
		pathToFileURL(`${SOURCE_ROOT}/tools/bash.ts`).href
	)) as { createBashTool: ToolFactory };
	const edit = (await import(
		pathToFileURL(`${SOURCE_ROOT}/tools/edit.ts`).href
	)) as { createEditTool: ToolFactory };
	const read = (await import(
		pathToFileURL(`${SOURCE_ROOT}/tools/read.ts`).href
	)) as { createReadTool: ToolFactory };
	const write = (await import(
		pathToFileURL(`${SOURCE_ROOT}/tools/write.ts`).href
	)) as { createWriteTool: ToolFactory };
	return {
		NodeExecutionEnv: env.NodeExecutionEnv,
		createReadTool: read.createReadTool,
		createWriteTool: write.createWriteTool,
		createEditTool: edit.createEditTool,
		createBashTool: bash.createBashTool,
	};
}

function schemas(apis: Apis): unknown[] {
	const tools = [
		apis.createReadTool(),
		apis.createWriteTool(),
		apis.createEditTool(),
		apis.createBashTool(),
	];
	return tools.map((tool) => ({
		name: tool.name,
		label: tool.label,
		description: tool.description,
		parameters: tool.parameters,
	}));
}

function createTool(
	apis: Apis,
	request: Extract<Request, { operation: "tool" }>,
): Tool {
	if (request.tool === "read" && request.imageProcessor) {
		return apis.createReadTool({
			autoResizeImages: false,
			imageProcessor: async () => ({
				ok: true,
				data: "converted",
				mimeType: "image/png",
				hints: ["[Image converted from image/bmp to image/png.]"],
			}),
		});
	}
	if (request.tool === "bash" && request.prepare) {
		return apis.createBashTool({
			commandPrefix: "prefix=ready",
			prepare: async (execution: {
				cwd: string;
				env: Record<string, string>;
				inheritEnv: boolean;
				command: string;
			}) => {
				execution.cwd = `${request.root}/workspace`;
				execution.env = { PI_BASH_PREPARE_EXPLICIT: "explicit" };
				execution.inheritEnv = false;
				execution.command +=
					'\nprintf \'%s:%s:%s:%s\' "$prefix" "${PI_BASH_PREPARE_INHERITED-}" "$PI_BASH_PREPARE_EXPLICIT" "$PWD"';
			},
		});
	}
	switch (request.tool) {
		case "read":
			return apis.createReadTool();
		case "write":
			return apis.createWriteTool();
		case "edit":
			return apis.createEditTool();
		case "bash":
			return apis.createBashTool();
	}
}

async function executeTool(
	apis: Apis,
	request: Extract<Request, { operation: "tool" }>,
): Promise<unknown> {
	const env = new apis.NodeExecutionEnv({ cwd: request.root });
	const controller = new AbortController();
	if (request.abort) controller.abort();
	const updates: unknown[] = [];
	const onUpdate = request.updates
		? (update: unknown) => updates.push(update)
		: undefined;
	const tool = createTool(apis, request);
	try {
		const result = await tool.execute(
			"call",
			request.input,
			controller.signal,
			onUpdate,
			{ env },
		);
		return {
			ok: true,
			content: result.content,
			details: result.details ?? null,
			updates,
		};
	} catch (error) {
		return {
			ok: false,
			error: error instanceof Error ? error.message : String(error),
			updates,
		};
	}
}

async function execute(apis: Apis, request: Request): Promise<unknown> {
	if (request.operation === "schemas") return schemas(apis);
	return await executeTool(apis, request);
}

const decoder = new StringDecoder("utf8");
let buffer = "";

async function main(): Promise<void> {
	const apis = await loadApis();
	process.stdin.on("data", async (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			let id = 0;
			try {
				const request = JSON.parse(line) as Request;
				id = request.id;
				const result = await execute(apis, request);
				const response: Response = { id, ok: true, result };
				process.stdout.write(`${JSON.stringify(response)}\n`);
			} catch (error) {
				const response: Response = {
					id,
					ok: false,
					error: {
						message: error instanceof Error ? error.message : String(error),
					},
				};
				process.stdout.write(`${JSON.stringify(response)}\n`);
			}
		}
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`harness-tools oracle startup failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
