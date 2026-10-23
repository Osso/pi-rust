import { rmdirSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { Agent } = await import(
	pathToFileURL(`${root}/../../agent-core/src/agent.ts`).href
);
const { AgentSession } = await import(
	pathToFileURL(`${root}/core/agent-session.ts`).href
);
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${root}/core/agent-session-runtime.ts`).href
);

type SessionReceiver = {
	continue(): Promise<void>;
	prompt(text: string): Promise<void>;
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
			getPromptCommand(name: string) {
				return name === "register-hello"
					? { invocationName: name, handler: async () => {} }
					: undefined;
			},
		},
		_withTurnStartLock: async (
			execute: (release: () => void) => Promise<void>,
		) => execute(() => {}),
		_flushPendingBashMessages() {},
		_isKnownSlashCommand() {
			return false;
		},
		_promptTurn: async () => {},
		_drainRuntimeCoordinationMessages: async () => {},
		_createCommandContext() {
			return {};
		},
		_recordHandledSlashCommandHistory() {},
	});
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as {
			cwd: string;
			operation: "continue" | "unknown" | "prompt" | "command" | "after-error";
		};
		if (request.operation === "after-error") {
			const agent = new Agent();
			const session = Object.create(AgentSession.prototype);
			Object.defineProperty(session, "agent", { value: agent });
			session._afterWorkingDirectoryRecoveryTurn = async () => {
				throw Object.assign(new RangeError(String.fromCharCode(0xd800)), {
					code: "ERECOVER",
				});
			};
			session._installAgentTurnStop();
			await agent.runWithLifecycle(async () => {
				await agent.shouldStopAfterTurn({});
			});
			const last = agent.state.messages.at(-1);
			const message = Array.from(
				{ length: last.errorMessage.length },
				(_, index) => last.errorMessage.charCodeAt(index),
			);
			console.log(
				JSON.stringify({ ok: true, stopReason: last.stopReason, message }),
			);
			continue;
		}
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
		try {
			switch (request.operation) {
				case "continue":
					await runtime._session.continue();
					break;
				case "unknown":
					await runtime._session.prompt("/not-a-command");
					break;
				case "prompt":
					await runtime._session.prompt("recovered prompt");
					break;
				case "command":
					await runtime._session.prompt("/register-hello");
					break;
			}
			console.log(JSON.stringify({ ok: true, cwd: runtime.cwd }));
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			console.log(
				JSON.stringify({ ok: false, cwd: runtime.cwd, message: error.message }),
			);
		}
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
