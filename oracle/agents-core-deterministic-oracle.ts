import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { runInThisContext } from "node:vm";

type RequestId = string | number | null;

type AgentSnapshot = Record<string, unknown> & {
	id: string;
	parentId?: string;
	lifecycle: string;
	displayName: string;
	agentType: string;
	revision: number;
};

type ToolResult = {
	content: Array<{ type: string; text?: string }>;
	details: Record<string, unknown>;
	isError?: boolean;
};

type StoreStub = {
	getAgent(agentId: string): AgentSnapshot | undefined;
	getActiveAgentCount(): number;
	listAgents(): AgentSnapshot[];
	listDescendants(parentId: string): AgentSnapshot[];
};

type Case = {
	name: string;
	op: string;
	agentId?: string;
	agent?: AgentSnapshot;
	agents?: AgentSnapshot[];
	activeCount?: number;
	body?: string;
	fromAgentId?: string;
	kind?: string;
	lifecycle?: string;
	message?: string;
	parentId?: string;
	params?: unknown;
	prompt?: string;
	sessionId?: string;
    storeSessionId?: string;
    trace?: boolean;
	toAgentId?: string;
};

type Request = { id: RequestId; cases: Case[] };

type Response =
	| { id: RequestId; ok: true; output: { results: Array<{ name: string; value: unknown }> } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

type Api = {
	emptyAgent(agentId: string): AgentSnapshot;
	isInFlightLifecycle(lifecycle: string): boolean;
	validateGoalObjective(prompt: string): unknown;
	spawnPromptValidationMessage(validation: unknown): string;
	backgroundPromptValidationMessage(validation: unknown): string;
	toolResult(text: string, details: Record<string, unknown>): ToolResult;
	errorToolResult(text: string, details: Record<string, unknown>): ToolResult;
	emptyToolResult(): ToolResult;
	listAgents(store: StoreStub, params: { parentId?: string }): ToolResult;
	agentViewer(store: StoreStub, params: { agentId: string; sessionId?: string; storeSessionId?: string; trace?: boolean }): ToolResult;
	registerAgentsCoreTools(pi: unknown, options: unknown): void;
	registerAllTools(pi: unknown, options: unknown): void;
	assertNoWaitAgentsParams(params: unknown, apiName: string): void;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function normalizeResult(result: ToolResult): unknown {
	return {
		content: result.content,
		details: result.details,
		isError: result.isError ?? null,
	};
}

function listDescendants(agents: AgentSnapshot[], parentId: string): AgentSnapshot[] {
	const descendants: AgentSnapshot[] = [];
	const visitChildren = (id: string): void => {
		for (const agent of agents) {
			if (agent.parentId !== id) continue;
			descendants.push(agent);
			visitChildren(agent.id);
		}
	};
	visitChildren(parentId);
	return descendants;
}

function storeStub(testCase: Case): StoreStub {
	const agents = testCase.agents ?? [];
	return {
		getActiveAgentCount: () => testCase.activeCount ?? agents.filter((agent) => isActive(agent.lifecycle)).length,
		getAgent: (agentId) => agents.find((agent) => agent.id === agentId),
		listAgents: () => agents,
		listDescendants: (parentId) => listDescendants(agents, parentId),
	};
}

function isActive(lifecycle: string): boolean {
	return lifecycle !== "completed" && lifecycle !== "failed" && lifecycle !== "aborted";
}

function executeCase(api: Api, testCase: Case): unknown {
	switch (testCase.op) {
		case "emptyAgent":
			return api.emptyAgent(requireString(testCase.agentId, "agentId"));
		case "isInFlightLifecycle":
			return api.isInFlightLifecycle(requireString(testCase.lifecycle, "lifecycle"));
		case "validateGoalObjective": {
			const validation = api.validateGoalObjective(requireString(testCase.prompt, "prompt"));
			return {
				validation,
				spawn: api.spawnPromptValidationMessage(validation),
				background: api.backgroundPromptValidationMessage(validation),
			};
		}
		case "toolResults":
			return {
				success: normalizeResult(api.toolResult("ok", { value: 1 })),
				failure: normalizeResult(api.errorToolResult("bad", { value: 2 })),
				empty: normalizeResult(api.emptyToolResult()),
			};
		case "listAgents":
			return normalizeResult(
				api.listAgents(storeStub(testCase), testCase.parentId === undefined ? {} : { parentId: testCase.parentId }),
			);
		case "agentViewer":
			return normalizeResult(
				api.agentViewer(storeStub(testCase), {
					agentId: requireString(testCase.agentId, "agentId"),
					...(testCase.sessionId === undefined ? {} : { sessionId: testCase.sessionId }),
                    ...(testCase.storeSessionId === undefined ? {} : { storeSessionId: testCase.storeSessionId }),
                    ...(testCase.trace === undefined ? {} : { trace: testCase.trace }),
				}),
			);
		case "toolMetadata": {
			const tools: Array<Record<string, unknown>> = [];
			const commands: Array<Record<string, unknown>> = [];
			const fakePi = {
				on: () => () => {},
				registerCommand: (name: string, definition: { description?: string }) => {
					commands.push({ name, description: definition.description ?? null });
				},
				registerTool: (tool: {
					name: string;
					label: string;
					description: string;
					promptGuidelines?: string[];
					parameters: unknown;
					approvalRequired?: boolean;
				}) => {
					tools.push({
						name: tool.name,
						label: tool.label,
						description: tool.description,
						promptGuidelines: tool.promptGuidelines ?? null,
						parameters: tool.parameters,
						approvalRequired: tool.approvalRequired ?? null,
					});
				},
			};
			api.registerAgentsCoreTools(fakePi, {});
			return { commands, tools };
		}
		case "allToolMetadata": {
			const tools: Array<Record<string, unknown>> = [];
			const commands: Array<Record<string, unknown>> = [];
			const fakePi = {
				on: () => () => {},
				registerCommand: (name: string, definition: { description?: string }) => {
					commands.push({ name, description: definition.description ?? null });
				},
				registerTool: (tool: {
					name: string;
					label: string;
					description: string;
					promptGuidelines?: string[];
					parameters: unknown;
					approvalRequired?: boolean;
				}) => {
					tools.push({
						name: tool.name,
						label: tool.label,
						description: tool.description,
						promptGuidelines: tool.promptGuidelines ?? null,
						parameters: tool.parameters,
						approvalRequired: tool.approvalRequired ?? null,
					});
				},
			};
			api.registerAllTools(fakePi, {});
			return { commands, tools };
		}
		case "assertNoWaitParams": {
			try {
				api.assertNoWaitAgentsParams(testCase.params ?? null, testCase.message ?? "wait_agent");
				return { ok: true };
			} catch (error) {
				return { ok: false, error: normalizeError(error).message };
			}
		}
		default:
			throw new Error(`unsupported agents-core case ${testCase.op}`);
	}
}

function requireString(value: string | undefined, name: string): string {
	if (typeof value !== "string") throw new Error(`${name} must be a string`);
	return value;
}

function execute(request: Request, api: Api): Response {
	try {
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((testCase) => ({
					name: testCase.name,
					value: executeCase(api, testCase),
				})),
			},
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: Api): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: Api): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("agents-core oracle requires a source module path");
	}
	return sourceModulePath;
}

// Frozen runtime.ts now owns the private helpers formerly split across modules.
// Expose declarations in a compiled in-memory copy; the frozen checkout stays immutable.
function loadFrozenModule<T>(file: string, privateExports: string[]): T {
	const ts = createRequire(file)("typescript") as typeof import("typescript");
	const source = readFileSync(file, "utf8");
	const exposed = `${source}\nexport { ${privateExports.join(", ")} };\n`;
	const compiled = ts.transpileModule(exposed, {
		compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
	}).outputText;
	const execute = runInThisContext(
		`(function(require, module, exports, __filename, __dirname) { ${compiled}\n})`,
		{ filename: file },
	) as (require: NodeRequire, module: { exports: unknown }, exports: unknown, file: string, directory: string) => void;
	const module = { exports: {} };
	execute(createRequire(file), module, module.exports, file, dirname(file));
	return module.exports as T;
}

async function main(): Promise<void> {
	const sourceRoot = resolve(readSourceModulePath());
	const runtime = loadFrozenModule<Api & {
		result: Api["toolResult"];
		errorResult: Api["errorToolResult"];
		emptyResult: Api["emptyToolResult"];
		default: Api["registerAllTools"];
	}>(`${sourceRoot}/runtime.ts`, [
		"emptyAgent", "isInFlightLifecycle", "validateGoalObjective",
		"spawnPromptValidationMessage", "backgroundPromptValidationMessage",
		"result", "errorResult", "emptyResult", "listAgents", "assertNoWaitAgentsParams",
	]);
	const viewer = loadFrozenModule<Pick<Api, "agentViewer">>(
		resolve(sourceRoot, "../../agent-viewer/src/runtime.ts"), ["agentViewer"],
	);
	serveRequests({
		emptyAgent: runtime.emptyAgent,
		isInFlightLifecycle: runtime.isInFlightLifecycle,
		validateGoalObjective: runtime.validateGoalObjective,
		spawnPromptValidationMessage: runtime.spawnPromptValidationMessage,
		backgroundPromptValidationMessage: runtime.backgroundPromptValidationMessage,
		toolResult: runtime.result,
		errorToolResult: runtime.errorResult,
		emptyToolResult: runtime.emptyResult,
		listAgents: runtime.listAgents,
		agentViewer: viewer.agentViewer,
		registerAgentsCoreTools: runtime.registerAgentsCoreTools,
		registerAllTools: runtime.default,
		assertNoWaitAgentsParams: runtime.assertNoWaitAgentsParams,
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`agents-core oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
