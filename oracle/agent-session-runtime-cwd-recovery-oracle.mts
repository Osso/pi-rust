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

type RecoveryNotice = { customType: string; content: string; display: boolean };

function createSession(
	cwd: string,
	recordCommand: (cwd: string) => void,
	recordNotice: (notice: RecoveryNotice) => void,
): SessionReceiver {
	return Object.assign(Object.create(AgentSession.prototype), {
		_extensionRunner: {
			setRelocateHandler() {},
			setToolResultRelocationHandlers() {},
			getPromptCommand(name: string) {
				return name === "record-cwd"
					? {
							invocationName: name,
							handler: async (_args: string, context: { cwd: string }) =>
								recordCommand(context.cwd),
						}
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
			return { cwd };
		},
		_recordHandledSlashCommandHistory() {},
		sendCustomMessage: async (notice: RecoveryNotice) => recordNotice(notice),
	});
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as {
			cwd: string;
			operation:
				| "continue"
				| "unknown"
				| "prompt"
				| "command"
				| "after-error"
				| "repair"
				| "streaming";
			deleted?: string;
		};
		if (request.operation === "streaming") {
			let commandCwd: string | undefined;
			const session = createSession(
				request.cwd,
				(value) => {
					commandCwd = value;
				},
				() => {},
			);
			Object.defineProperties(session, {
				isStreaming: { value: true },
				messages: { value: [] },
			});
			Object.assign(session, {
				_beforeWorkingDirectoryRecoveryRun: async () => {
					throw new Error("must not run");
				},
			});
			let message: string | undefined;
			try {
				await session.continue();
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				message = error.message;
			}
			await session.prompt("/record-cwd");
			console.log(JSON.stringify({ message, commandCwd }));
			continue;
		}
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
		let commandCwd: string | undefined;
		let notice: RecoveryNotice | undefined;
		const newSession = (cwd: string) => {
			const session = createSession(
				cwd,
				(value) => {
					commandCwd = value;
				},
				(value) => {
					notice = value;
				},
			);
			Object.defineProperties(session, {
				isStreaming: { value: false },
				messages: { value: [] },
			});
			return session;
		};
		runtime._services = { cwd: request.cwd };
		runtime._session = newSession(request.cwd);
		runtime.relocate = async (cwd) => {
			runtime._services = { cwd };
			runtime._session = newSession(cwd);
			runtime.bindSessionRuntimeActions();
		};
		runtime.bindSessionRuntimeActions();
		if (request.operation === "repair") {
			if (request.deleted === undefined)
				throw new Error("deleted fixture required");
			const original = process.cwd();
			try {
				process.chdir(request.deleted);
				rmdirSync(request.deleted);
				await runtime._session.continue();
				console.log(JSON.stringify({ ok: true, cwd: process.cwd(), notice }));
			} finally {
				process.chdir(original);
			}
			continue;
		}
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
					await runtime._session.prompt("/record-cwd");
					break;
			}
			console.log(
				JSON.stringify({
					ok: true,
					cwd: runtime.cwd,
					...(request.operation === "command" ? { commandCwd } : {}),
				}),
			);
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
