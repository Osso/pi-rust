// Execute the frozen extension's registered tool, never a copied normalization helper.
import { createInterface } from "node:readline";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
const source = process.argv[2];
const { default: register } = await import(pathToFileURL(source).href);
const core = resolve(dirname(source), "../../../src/core");

async function scenario(request: any): Promise<unknown> {
	let name: string | undefined = request.initialName ?? undefined;
	let stored = request.initialName !== undefined;
	const events: unknown[] = [];
	const handlers = new Map<string, Function>();
	let tool: any;
	let requests = 0;
	let setterError: string | undefined;
	const manager = {
		getSessionFile: () => request.ephemeral ? undefined : "/fixture/session.jsonl",
		isSubagentSession: () => Boolean(request.subagent),
		getSessionName: () => name,
		hasSessionNameState: () => stored,
	};
	const ctx = {
		sessionManager: manager,
		multiAgentAgentId: request.agentId,
		get modelRegistry() { requests++; throw new Error("unexpected title request"); },
	};
	const api = {
		registerTool: (definition: any) => { tool = definition; },
		on: (event: string, handler: Function) => handlers.set(event, handler),
		setSessionName: (next: string) => {
			if (setterError) throw new Error(setterError);
			name = next; stored = true;
			events.push({ type: "session_info_changed", name });
		},
	};
	register(api);
	const metadata = {
		name: tool.name, label: tool.label, description: tool.description,
		promptGuidelines: tool.promptGuidelines, approvalRequired: tool.approvalRequired,
		parameters: tool.parameters,
	};
	const output = [];
	for (const step of request.steps) {
		// Encode UTF16 units in transport: readline also recognizes Unicode line separators.
		if (step.nameUnits) step.name = String.fromCharCode(...step.nameUnits);
		let result;
		let error;
		if (step.op === "external") { name = step.name ?? undefined; stored = true; }
		if (step.op === "reset") { handlers.clear(); register(api); }
		if (step.op === "newSession") { name = undefined; stored = false; }
		if (step.op === "setterError") setterError = step.error;
		if (step.op === "session_start") await handlers.get("session_start")?.({ type: "session_start" }, ctx);
		if (step.op === "call") {
			try { result = await tool.execute("fixture-call", { name: step.name }, undefined, undefined, ctx); }
			catch (caught) { error = (caught as Error).message; }
		}
		output.push({ result: result ?? null, error: error ?? null, name: name ?? null, stored, events: [...events] });
	}
	// JSON text preserves split-surrogate fields without coercion by a UTF-8 JSON parser.
	return { metadata, outputJson: JSON.stringify(output), titleRequests: requests };
}

async function persistence(request: any): Promise<unknown> {
	const { SessionManager } = await import(pathToFileURL(resolve(core, "session-manager.ts")).href);
	const db = await import(pathToFileURL(resolve(core, "session-control-db.ts")).href);
	let manager;
	if (request.op === "create" || request.op === "toolPersist") {
		manager = SessionManager.create(request.cwd, request.sessionDir);
		manager.setMetadataControlDbPath(request.db);
		manager.persistForRecovery();
		if (request.name !== undefined) manager.setSessionName(request.name ?? undefined);
	} else {
		manager = SessionManager.open(request.path, request.sessionDir);
		manager.setMetadataControlDbPath(request.db);
	}
	if (request.op === "toolPersist") {
		let tool: any;
		register({
			registerTool: (definition: any) => { tool = definition; },
			setSessionName: (name: string) => manager.setSessionName(name),
		});
		const result = await tool.execute("persist-call", { name: request.toolName }, undefined, undefined, { sessionManager: manager });
		const state = db.readSessionNameState(request.db, manager.getSessionFile());
		return {
			path: manager.getSessionFile(), resultJson: JSON.stringify(result),
			managerNameJson: JSON.stringify(manager.getSessionName()),
			storedNameJson: JSON.stringify(state.name),
		};
	}
	return {
		path: manager.getSessionFile(), name: manager.getSessionName() ?? null,
		stored: manager.hasSessionNameState(),
		state: db.readSessionNameState(request.db, manager.getSessionFile()),
	};
}

for await (const line of createInterface({ input: process.stdin })) {
	if (!line.trim()) continue;
	try {
		const request = JSON.parse(line);
		const output = request.op ? await persistence(request) : await scenario(request);
		process.stdout.write(`${JSON.stringify(output)}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ error: (error as Error).stack })}\n`);
	}
}

}
void main();
