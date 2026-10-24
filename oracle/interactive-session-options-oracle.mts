import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(
	pathToFileURL(`${source}/modes/interactive/interactive-mode.ts`).href
);
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${source}/core/agent-session-runtime.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${source}/core/session-manager.ts`).href
);
interface Request {
	root: string;
	operation: "switch" | "fork";
	position?: "at" | "before";
	seed: string;
}
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		const manager = SessionManager.create(
			request.root,
			join(request.root, "sessions"),
		);
		manager.setMetadataControlDbPath(join(request.root, "control.db"));
		const entryId = manager.appendMessage({
			role: "user",
			content: request.seed,
			timestamp: 0,
		});
		manager.appendMessage({
			role: "assistant",
			content: [],
			api: "faux",
			provider: "faux",
			model: "faux-1",
			usage: {
				input: 0,
				output: 0,
				cacheRead: 0,
				cacheWrite: 0,
				totalTokens: 0,
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
			},
			stopReason: "stop",
			timestamp: 1,
		});
		let actions:
			| {
					switchSession: (
						path: string,
						options: unknown,
					) => Promise<{ cancelled: boolean }>;
					fork: (
						entryId: string,
						options: unknown,
					) => Promise<{ cancelled: boolean }>;
			  }
			| undefined;
		const makeSession = (sm: typeof manager) => ({
			sessionManager: sm,
			sessionFile: sm.getSessionFile(),
			agent: { state: { messages: sm.buildSessionContext().messages } },
			resourceLoader: { getThemes: () => ({ themes: [] }) },
			extensionRunner: {},
			createReplacedSessionContext: () => ({}),
			bindExtensions: async (bindings: {
				commandContextActions: typeof actions;
			}) => {
				actions = bindings.commandContextActions;
			},
		});
		const runtime = Object.create(AgentSessionRuntime.prototype);
		runtime.lifecycleTransitionTail = Promise.resolve();
		runtime._session = makeSession(manager);
		runtime._services = { cwd: request.root, agentDir: request.root };
		runtime.emitBeforeSwitch = async () => ({ cancelled: false });
		runtime.emitBeforeFork = async () => ({ cancelled: false });
		runtime.teardownCurrent = async () => {};
		runtime.createRuntime = async (options: {
			cwd: string;
			sessionManager: typeof manager;
		}) => ({
			session: makeSession(options.sessionManager),
			services: { cwd: options.cwd, agentDir: request.root },
		});
		runtime.apply = (result: { session: unknown; services: unknown }) => {
			runtime._session = result.session;
			runtime._services = result.services;
		};
		const receiver = Object.create(InteractiveMode.prototype);
		receiver.runtimeHost = runtime;
		receiver.options = {};
		receiver.footerDataProvider = {};
		receiver.statusContainer = { clear: () => {} };
		receiver.createExtensionUIContext = () => ({
			select: () => {},
			confirm: () => {},
			input: () => {},
			notify: () => {},
		});
		receiver.setupAutocompleteProvider = () => {};
		receiver.setupExtensionShortcuts = () => {};
		receiver.showLoadedResources = () => {};
		receiver.showStartupNoticesIfNeeded = () => {};
		receiver.continueInterruptedResumedSession = async () => {};
		receiver.showStatus = () => {};
		receiver.handleFatalRuntimeError = (_label: string, error: unknown) => {
			throw error;
		};
		let editor = "editor before";
		receiver.editor = {
			setText: (text: string) => {
				editor = text;
			},
		};
		receiver.pendingCompactionSummaryEditId = "stale";
		receiver.liveCompactionDisplay = { entryId: "stale" };
		await receiver.bindCurrentSessionExtensions();
		if (!actions) throw new Error("source did not bind actions");
		let called = 0;
		const withSession = async () => {
			called++;
		};
		let result: { cancelled: boolean };
		if (request.operation === "switch") {
			const target = SessionManager.create(
				request.root,
				join(request.root, "target"),
			);
			target.persistForRecovery();
			result = await actions.switchSession(target.getSessionFile(), {
				withSession,
			});
		} else {
			result = await actions.fork(entryId, {
				position: request.position,
				withSession,
			});
			if (receiver.liveCompactionDisplay !== undefined)
				throw new Error("source failed to clear live compaction display");
		}
		console.log(
			JSON.stringify({
				called,
				cancelled: result.cancelled,
				messageCount:
					runtime.session.sessionManager.buildSessionContext().messages.length,
				...(request.operation === "fork"
					? {
							editor,
							summaryCleared:
								receiver.pendingCompactionSummaryEditId === undefined,
							liveCleared: receiver.liveCompactionDisplay === undefined,
						}
					: {}),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ error: String(error) }));
	}
}
