import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Model = { id: string; provider: string };
type Op = { kind: string; args?: string; model?: Model; payload?: unknown };
type Case = { model?: Model; child?: string; requiresId?: boolean; branch?: unknown[]; ops: Op[] };
type Context = {
	model?: Model;
	multiAgentAgentId?: string;
	multiAgentRequiresAgentId?: boolean;
	sessionManager: { getBranch(): unknown[] };
	ui: { notify(message: string, kind: string): void; setStatus(key: string, value?: string): void; setEditorText(text: string): void };
};
type Command = { description: string; handler(args: string, ctx: Context): Promise<void> };
type Handler = (event: { model?: Model; payload?: unknown }, ctx: Context) => unknown;
type Api = { registerCommand(name: string, command: Command): void; on(name: string, handler: Handler): void; appendEntry(name: string, data: unknown): void };

async function runCase(register: (pi: Api) => void, input: Case): Promise<unknown> {
	const commands = new Map<string, Command>();
	const handlers = new Map<string, Handler>();
	const entries: unknown[] = [];
	const notifications: unknown[] = [];
	const statuses: unknown[] = [];
	const editor: string[] = [];
	const payloads: unknown[] = [];
	register({
		registerCommand(name, command) { commands.set(name, command); },
		on(name, handler) { handlers.set(name, handler); },
		appendEntry(name, data) { entries.push([name, data]); },
	});
	const ctx: Context = {
		model: input.model, multiAgentAgentId: input.child, multiAgentRequiresAgentId: input.requiresId,
		sessionManager: { getBranch: () => input.branch ?? [] },
		ui: {
			notify(message, kind) { notifications.push([message, kind]); },
			setStatus(key, value) { statuses.push([key, value ?? null]); },
			setEditorText(text) { editor.push(text); },
		},
	};
	const command = commands.get("daybreak");
	if (command === undefined) throw new Error("daybreak command missing");
	for (const op of input.ops) {
		if (op.kind === "command") await command.handler(op.args ?? "", ctx);
		else {
			const handler = handlers.get(op.kind);
			if (handler === undefined) throw new Error(`missing ${op.kind}`);
			if (op.kind === "model_select") ctx.model = op.model;
			const result = await handler({ model: op.model, payload: op.payload }, ctx);
			if (op.kind === "before_provider_request") payloads.push(result ?? null);
		}
	}
	return { description: command.description, entries, notifications, statuses, editor, payloads };
}

runOracle(async (api, request) => {
	const register = api.default as (pi: Api) => void;
	return await Promise.all(requireCases(request).map(value => runCase(register, value as Case)));
}).catch(error => { console.error(error); process.exitCode = 1; });
