import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { fileURLToPath, pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
if (process.argv[3] === "--child") {
	const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
	const { AgentSessionRuntime } = await import(pathToFileURL(`${root}/core/agent-session-runtime.ts`).href);
	const noop = () => {};
	const sessionManager = { getSessionFile: () => "/fixture/session.jsonl", isPersisted: () => true,
		setSessionThinkingLevel: noop, persistForRecovery: noop, getMetadataControlDbPath: () => undefined };
	const session = { sessionManager, settingsManager: { getShowTerminalProgress: () => false },
		detachAllRunningTools: noop, thinkingLevel: "off", extensionRunner: { hasHandlers: () => false }, dispose: noop };
	const runtime = Object.assign(Object.create(AgentSessionRuntime.prototype), {
		_session: session, lifecycleTransitionTail: Promise.resolve(),
	});
	const mode = Object.assign(Object.create(InteractiveMode.prototype), {
		runtimeHost: runtime, signalCleanupHandlers: [], isInitialized: true,
		themeController: { disableAutoSync: noop },
		ui: { terminal: { drainInput: async () => {} }, stop: () => process.stdout.write("STOPPED\n") },
		stopChildActivityTimer: noop, clearChildAgentView: noop, clearExtensionTerminalInputListeners: noop,
		footer: { dispose: noop }, footerDataProvider: { dispose: noop }, cancelPartialUpdateRender: noop,
		disposeActiveBashComponents: noop, clearPendingToolComponents: noop, closeResponseCompleteNotification: noop,
		clipboardTempFiles: { cleanupAll: noop },
	});
	runtime.setBeforeProcessRestart(() => mode.stopInteractiveTerminalForProcessRestart());
	runtime.setProcessRestarter(async () => { throw new Error("fixture restart failure"); });
	mode.registerSignalHandlers();
	process.emit("SIGHUP");
} else {
	for await (const _line of createInterface({ input: process.stdin })) {
		try {
			let output = "";
			const child = spawn(process.execPath, [...process.execArgv, fileURLToPath(import.meta.url), root, "--child"], { stdio: ["ignore", "pipe", "pipe"] });
			child.stdout.on("data", (bytes) => { output += bytes.toString(); });
			child.stderr.on("data", (bytes) => { output += bytes.toString(); });
			const code = await new Promise<number | null>((resolve, reject) => { child.on("error", reject); child.on("close", resolve); });
			const stopped = output.indexOf("STOPPED");
			const error = output.indexOf("Error: fixture restart failure");
			if (stopped < 0 || error < 0) throw new Error(`frozen fixture did not reach stop/restart failure: ${output}`);
			console.log(JSON.stringify({ ok: true, output: { code, banner: output.includes("pi exiting due to uncaughtException:"), reported: error >= 0, restoredBeforeReport: stopped < error } }));
		} catch (error) {
			console.log(JSON.stringify({ ok: false, error: String(error) }));
		}
	}
}
