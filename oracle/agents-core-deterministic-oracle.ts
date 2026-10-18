import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

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
	prompt?: string;
	sessionId?: string;
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
	createFailedMailboxMessage(input: {
		body: string;
		fromAgentId: string;
		kind: string;
		toAgentId: string;
	}): unknown;
	toolResult(text: string, details: Record<string, unknown>): ToolResult;
	errorToolResult(text: string, details: Record<string, unknown>): ToolResult;
	emptyToolResult(): ToolResult;
	listAgents(store: StoreStub, params: { parentId?: string }): ToolResult;
	agentViewer(store: StoreStub, params: { agentId: string; sessionId?: string }): ToolResult;
	appendParentAgentStart(pi: { appendEntry(type: string, data: unknown): void }, agent: AgentSnapshot): void;
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
		case "createFailedMailboxMessage":
			return api.createFailedMailboxMessage({
				body: requireString(testCase.body, "body"),
				fromAgentId: requireString(testCase.fromAgentId, "fromAgentId"),
				kind: requireString(testCase.kind, "kind"),
				toAgentId: requireString(testCase.toAgentId, "toAgentId"),
			});
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
				}),
			);
		case "appendParentAgentStart": {
			const entries: Array<{ type: string; data: unknown }> = [];
			api.appendParentAgentStart(
				{ appendEntry: (type, data) => entries.push({ type, data }) },
				testCase.agent as AgentSnapshot,
			);
			return entries;
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

async function main(): Promise<void> {
	const sourceRoot = readSourceModulePath();
	const shared = await import(pathToFileURL(`${sourceRoot}/shared.ts`).href);
	const toolResults = await import(pathToFileURL(`${sourceRoot}/tool-results.ts`).href);
	const runtimeUtils = await import(pathToFileURL(`${sourceRoot}/runtime-utils.ts`).href);
	const sessionFactories = await import(pathToFileURL(`${sourceRoot}/session-factories.ts`).href);
	const viewer = await import(pathToFileURL(`${sourceRoot}/viewer.ts`).href);
	const parentAgentJournal = await import(pathToFileURL(`${sourceRoot}/parent-agent-journal.ts`).href);
	serveRequests({
		emptyAgent: shared.emptyAgent,
		isInFlightLifecycle: shared.isInFlightLifecycle,
		validateGoalObjective: sessionFactories.validateGoalObjective,
		spawnPromptValidationMessage: sessionFactories.spawnPromptValidationMessage,
		backgroundPromptValidationMessage: sessionFactories.backgroundPromptValidationMessage,
		createFailedMailboxMessage: runtimeUtils.createFailedMailboxMessage,
		toolResult: toolResults.toolResult,
		errorToolResult: toolResults.errorToolResult,
		emptyToolResult: toolResults.emptyToolResult,
		listAgents: viewer.listAgents,
		agentViewer: viewer.agentViewer,
		appendParentAgentStart: parentAgentJournal.appendParentAgentStart,
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`agents-core oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
