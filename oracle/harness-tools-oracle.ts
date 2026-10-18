import { NodeExecutionEnv } from "/home/osso/.worktrees/pi-rust-source-532f7d12/packages/agent/src/harness/env/nodejs.ts";
import { createBashTool } from "/home/osso/.worktrees/pi-rust-source-532f7d12/packages/agent/src/harness/tools/bash.ts";
import { createEditTool } from "/home/osso/.worktrees/pi-rust-source-532f7d12/packages/agent/src/harness/tools/edit.ts";
import { createReadTool } from "/home/osso/.worktrees/pi-rust-source-532f7d12/packages/agent/src/harness/tools/read.ts";
import { createWriteTool } from "/home/osso/.worktrees/pi-rust-source-532f7d12/packages/agent/src/harness/tools/write.ts";
import { StringDecoder } from "node:string_decoder";

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

type Response = { id: number; ok: true; result: unknown } | { id: number; ok: false; error: { message: string } };

const tools = {
	read: createReadTool(),
	write: createWriteTool(),
	edit: createEditTool(),
	bash: createBashTool(),
};

type AnyTool = {
	execute(
		toolCallId: string,
		params: unknown,
		signal: AbortSignal | undefined,
		onUpdate: ((update: unknown) => void) | undefined,
		context: { env: NodeExecutionEnv },
	): Promise<{ content: unknown[]; details?: unknown }>;
};

function asAnyTool(tool: unknown): AnyTool {
	return tool as AnyTool;
}

function schemas(): unknown[] {
	return Object.values(tools).map((tool) => ({
		name: tool.name,
		label: tool.label,
		description: tool.description,
		parameters: tool.parameters,
	}));
}

function createTool(request: Extract<Request, { operation: "tool" }>): AnyTool {
	if (request.tool === "read" && request.imageProcessor) {
		return asAnyTool(createReadTool({
			autoResizeImages: false,
			imageProcessor: async () => ({
				ok: true,
				data: "converted",
				mimeType: "image/png",
				hints: ["[Image converted from image/bmp to image/png.]"],
			}),
		}));
	}
	if (request.tool === "bash" && request.prepare) {
		return asAnyTool(createBashTool({
			commandPrefix: "prefix=ready",
			prepare: async (execution) => {
				execution.cwd = `${request.root}/workspace`;
				execution.env = { PI_BASH_PREPARE_EXPLICIT: "explicit" };
				execution.inheritEnv = false;
				execution.command +=
					"\nprintf '%s:%s:%s:%s' \"$prefix\" \"${PI_BASH_PREPARE_INHERITED-}\" \"$PI_BASH_PREPARE_EXPLICIT\" \"$PWD\"";
			},
		}));
	}
	return asAnyTool(tools[request.tool]);
}

async function executeTool(request: Extract<Request, { operation: "tool" }>): Promise<unknown> {
	const env = new NodeExecutionEnv({ cwd: request.root });
	const controller = new AbortController();
	if (request.abort) controller.abort();
	const updates: unknown[] = [];
	const onUpdate = request.updates ? (update: unknown) => updates.push(update) : undefined;
	const tool = createTool(request);
	try {
		const result = await tool.execute("call", request.input, controller.signal, onUpdate, { env });
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

async function execute(request: Request): Promise<unknown> {
	if (request.operation === "schemas") return schemas();
	return await executeTool(request);
}

const decoder = new StringDecoder("utf8");
let buffer = "";
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
			const result = await execute(request);
			const response: Response = { id, ok: true, result };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		} catch (error) {
			const response: Response = {
				id,
				ok: false,
				error: { message: error instanceof Error ? error.message : String(error) },
			};
			process.stdout.write(`${JSON.stringify(response)}\n`);
		}
	}
});
