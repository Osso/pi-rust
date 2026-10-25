import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Step = { name?: string; externalName?: string | null; fail?: boolean };
type Case = {
	persisted?: boolean;
	child?: string;
	subagent?: boolean;
	initialName?: string | null;
	steps: Step[];
};
type Result = { content: unknown; details: unknown; isError?: boolean };
type NamingTool = {
	name: string;
	label: string;
	description: string;
	promptGuidelines: string[];
	approvalRequired: boolean;
	parameters: unknown;
	execute(id: string, params: { name: string }, signal: undefined, update: undefined, ctx: unknown): Promise<Result>;
};
type Api = { registerTool(tool: NamingTool): void; setSessionName(name: string): void };

async function runCase(register: (api: Api) => void, input: Case): Promise<unknown> {
	let tool: NamingTool | undefined;
	let currentName = input.initialName ?? undefined;
	let hasNameState = input.initialName !== undefined;
	let fail = false;
	const writes: string[] = [];
	register({
		registerTool(value) { tool = value; },
		setSessionName(name) {
			if (fail) throw new Error("name write failed");
			writes.push(name);
			currentName = name;
			hasNameState = true;
		},
	});
	if (tool === undefined) throw new Error("set_session_name tool missing");
	const ctx = {
		multiAgentAgentId: input.child,
		sessionManager: {
			isSubagentSession: () => input.subagent ?? false,
			getSessionFile: () => input.persisted === false ? undefined : "/private/session.jsonl",
			getSessionName: () => currentName,
			hasSessionNameState: () => hasNameState,
		},
	};
	const results: unknown[] = [];
	for (const step of input.steps) {
		if (Object.hasOwn(step, "externalName")) {
			currentName = step.externalName ?? undefined;
			hasNameState = true;
			continue;
		}
		fail = step.fail ?? false;
		try { results.push(await tool.execute("name", { name: step.name ?? "" }, undefined, undefined, ctx)); }
		catch (error) {
			if (!(error instanceof Error)) throw error;
			results.push({ error: error.message });
		}
	}
	const { execute, ...metadata } = tool;
	return { metadata, results, writes, currentName: currentName ?? null, hasNameState };
}

runOracle(async (api, request) => {
	const register = api.default as (api: Api) => void;
	return await Promise.all(requireCases(request).map(value => runCase(register, value as Case)));
}).catch(error => { console.error(error); process.exitCode = 1; });
