import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${source}/core/agent-session-runtime.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${source}/core/session-manager.ts`).href
);
interface Request {
	root: string;
	operation:
		| "setup"
		| "withSession"
		| "beforeRestart"
		| "asyncSetup"
		| "asyncSetupReject";
	kind: "native" | "raw";
	value: unknown;
}
function errorWire(error: unknown) {
	if (error instanceof Error) {
		const native = error as Error & { code?: unknown; custom?: unknown };
		return {
			kind: "error",
			class: error instanceof RangeError ? "RangeError" : "Error",
			value: {
				name: native.name,
				message: native.message,
				code: native.code,
				custom: native.custom,
			},
		};
	}
	return { kind: "value", value: error };
}
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		const details = request.value as {
			message: string;
			code?: unknown;
			custom?: unknown;
		};
		const failure =
			request.kind === "native"
				? Object.assign(new RangeError(details.message), {
						code: details.code,
						custom: details.custom,
					})
				: request.value;
		const manager = SessionManager.create(
			request.root,
			join(request.root, "sessions"),
		);
		manager.setMetadataControlDbPath(join(request.root, "control.db"));
		manager.persistForRecovery();
		const calls: string[] = [];
		const runtime = Object.create(AgentSessionRuntime.prototype);
		runtime.lifecycleTransitionTail = Promise.resolve();
		runtime._session = {
			sessionManager: manager,
			sessionFile: manager.getSessionFile(),
			model: { provider: "faux", id: "faux-1" },
			thinkingLevel: "off",
			agent: { state: { messages: [] } },
			detachAllRunningTools: () => {
				calls.push("detach");
			},
		};
		runtime._services = { cwd: request.root, agentDir: request.root };
		runtime.emitBeforeSwitch = async () => ({ cancelled: false });
		runtime.teardownCurrent = async () => {
			calls.push("teardown");
		};
		runtime.createRuntime = async (options: {
			sessionManager: typeof manager;
		}) => {
			calls.push("create");
			return {
				session: {
					sessionManager: options.sessionManager,
					agent: { state: { messages: [] } },
					createReplacedSessionContext: () => ({}),
				},
			};
		};
		runtime.apply = (result: { session: unknown }) => {
			runtime._session = result.session;
		};
		runtime.rebindSession = async () => {
			calls.push("rebind");
		};
		runtime.beforeProcessRestart = async () => {
			calls.push("beforeRestart");
			throw failure;
		};
		runtime.processRestarter = async () => {
			calls.push("restarter");
			throw new Error("unexpected restarter");
		};
		let error: unknown;
		try {
			if (request.operation === "beforeRestart")
				await runtime.restart({ process: true });
			else
				await runtime.newSession({
					setup: async (sm: typeof manager) => {
						calls.push("setup");
						sm.appendCustomMessageEntry("setup_visible", "setup payload", true);
						if (request.operation.startsWith("asyncSetup")) {
							await Promise.resolve();
							calls.push("afterAwait");
							sm.appendCustomMessageEntry(
								"setup_after_await",
								"after await",
								true,
							);
						}
						if (
							request.operation === "setup" ||
							request.operation === "asyncSetupReject"
						)
							throw failure;
					},
					withSession: async () => {
						calls.push("withSession");
						if (request.operation === "withSession") throw failure;
					},
				});
		} catch (caught) {
			error = caught;
		}
		const setupVisible = runtime.session.sessionManager
			.getEntries()
			.some(
				(entry: { customType?: string }) =>
					entry.customType === "setup_visible",
			);
		console.log(
			JSON.stringify({
				calls,
				error: errorWire(error),
				setupVisible,
				agentMessageCount: runtime.session.agent.state.messages.length,
				...(request.operation.startsWith("asyncSetup")
					? {
							setupEntries: runtime.session.sessionManager
								.getEntries()
								.filter(
									(entry: { type: string }) => entry.type === "custom_message",
								)
								.map((entry: { customType: string; content: unknown }) => ({
									customType: entry.customType,
									content: entry.content,
								})),
						}
					: {}),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ harnessError: String(error) }));
	}
}
