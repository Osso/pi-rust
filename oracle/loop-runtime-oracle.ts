import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Message = { role?: string; customType: string; content: string; display: boolean; details: { generation: number; sessionId: string } };
type Context = { isIdle(): boolean; hasPendingMessages(): boolean; sessionManager: { getSessionId(): string }; ui: { notify(): void; setEditorText(): void } };
type Handler = (event: { messages?: Message[]; sessionContinuation?: string }, ctx: Context) => unknown;
type Api = {
	on(name: string, handler: Handler): void;
	registerCommand(name: string, command: { handler(args: string, ctx: Context): Promise<void> }): void;
	registerTool(tool: unknown): void;
	sendMessage(message: Message, options: unknown): void;
};
type Op = { kind: string; args?: string; idle?: boolean; pending?: boolean; sessionId?: string; include?: boolean; continuation?: boolean; old?: boolean };

async function runCase(register: (pi: Api) => void, ops: Op[]): Promise<unknown> {
	const timers = new Map<number, () => void>();
	const handlers = new Map<string, Handler>();
	let command: ((args: string, ctx: Context) => Promise<void>) | undefined;
	let idle = true;
	let pending = false;
	let sessionId = "00000000-0000-4000-8000-000000000001";
	let nextTimer = 0;
	const sends: [Message, unknown][] = [];
	const steps: unknown[] = [];
	const originalSet = globalThis.setInterval;
	const originalClear = globalThis.clearInterval;
	globalThis.setInterval = ((callback: () => void) => { const id = ++nextTimer; timers.set(id, callback); return id; }) as unknown as typeof setInterval;
	globalThis.clearInterval = ((id: number) => { timers.delete(id); }) as typeof clearInterval;
	try {
		register({
			on(name, handler) { handlers.set(name, handler); },
			registerCommand(_name, value) { command = value.handler; },
			registerTool() {},
			sendMessage(message, options) { sends.push([message, options]); },
		});
		const ctx: Context = { isIdle: () => idle, hasPendingMessages: () => pending, sessionManager: { getSessionId: () => sessionId }, ui: { notify() {}, setEditorText() {} } };
		if (command === undefined) throw new Error("loop command missing");
		for (const op of ops) {
			let error: string | null = null;
			try {
				if (op.kind === "command") await command(op.args ?? "", ctx);
				if (op.kind === "tick") for (const callback of timers.values()) callback();
				if (op.kind === "state") { idle = op.idle ?? idle; pending = op.pending ?? pending; sessionId = op.sessionId ?? sessionId; }
				if (op.kind === "end") {
					const message = op.old ? sends[0]?.[0] : sends.at(-1)?.[0];
					await handlers.get("agent_end")?.({ messages: op.include && message ? [{ role: "custom", ...message }] : [], sessionContinuation: op.continuation ? "cwd-relocation" : undefined }, ctx);
				}
				if (op.kind === "shutdown") await handlers.get("session_shutdown")?.({}, ctx);
			} catch (caught) { if (!(caught instanceof Error)) throw caught; error = caught.message; }
			steps.push({ sent: sends.length, error });
		}
		return { steps, sends };
	} finally { globalThis.setInterval = originalSet; globalThis.clearInterval = originalClear; }
}

runOracle(async (api, request) => {
	const register = api.default as (pi: Api) => void;
	const outputs = [];
	for (const ops of requireCases(request)) outputs.push(await runCase(register, ops as Op[]));
	return outputs;
}).catch(error => { console.error(error); process.exitCode = 1; });
