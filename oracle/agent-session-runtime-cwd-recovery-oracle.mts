import { rmdirSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { AgentSession } = await import(
	pathToFileURL(`${root}/core/agent-session.ts`).href
);
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${root}/core/agent-session-runtime.ts`).href
);

type SessionReceiver = {
	continue(): Promise<void>;
};
type RuntimeReceiver = {
	_session: SessionReceiver;
	_services: { cwd: string };
	readonly cwd: string;
	bindSessionRuntimeActions(): void;
	relocate(cwd: string): Promise<void>;
};

function createSession(): SessionReceiver {
	return Object.assign(Object.create(AgentSession.prototype), {
		_extensionRunner: {
			setRelocateHandler() {},
			setToolResultRelocationHandlers() {},
		},
		_withTurnStartLock: async (
			execute: (release: () => void) => Promise<void>,
		) => execute(() => {}),
		_flushPendingBashMessages() {},
	});
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as { cwd: string };
		const runtime = Object.create(
			AgentSessionRuntime.prototype,
		) as RuntimeReceiver;
		const newSession = () => {
			const session = createSession();
			Object.defineProperties(session, {
				isStreaming: { value: false },
				messages: { value: [] },
			});
			return session;
		};
		runtime._services = { cwd: request.cwd };
		runtime._session = newSession();
		runtime.relocate = async (cwd) => {
			runtime._services = { cwd };
			runtime._session = newSession();
			runtime.bindSessionRuntimeActions();
		};
		runtime.bindSessionRuntimeActions();
		rmdirSync(request.cwd);
		await runtime._session.continue();
		console.log(JSON.stringify({ ok: true, cwd: runtime.cwd }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
