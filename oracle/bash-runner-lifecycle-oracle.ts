import { existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";
type Input = { mode: string; command: string; timeout?: number };
type Registry = { detachRunning(): boolean; hasRunning(): boolean };
type Api = {
	BashToolDetachRegistry: new () => Registry;
	createBashToolDefinition(cwd: string, options: unknown): {
		execute(id: string, args: unknown, signal: AbortSignal, onUpdate: (value: { content: Array<{ text?: string }> }) => void): Promise<unknown>;
	};
};
const helpers = (async () => {
	const directory = join(dirname(process.argv[2]), "..");
	const load = (file: string) => import(pathToFileURL(join(directory, file)).href);
	const [lifecycle, coordinator, store, runtimeProcess] = await Promise.all([load("detached-job-lifecycle.ts"), load("lifecycle-coordinator.ts"), load("multi-agent-store.ts"), load("runtime-process.ts")]);
	return { lifecycle, coordinator, store, runtimeProcess };
})();
void runOracle(async (module, request) => {
	const api = module as Api;
	const h = await helpers;
	const output = [];
	for (const [index, value] of requireCases(request).entries()) {
		const input = value as Input;
		const root = join(String(request.cwd), `source-${index}`);
		mkdirSync(root);
		const controlDbPath = join(root, "control.sqlite");
		const sessionPath = join(root, "session.jsonl");
		const store = new h.store.MultiAgentStore();
		const coordinator = new h.coordinator.LifecycleCoordinator({ controlDbPath, sessionPath, createAgentId: () => "unused", now: () => "2026-10-24T00:00:00.000Z", processIdentity: h.runtimeProcess.readProcessIdentity(process.pid) });
		const lifecycle = h.lifecycle.createDetachedJobLifecycleController({ artifactRoot: root, controlDbPath, sessionPath, coordinator, store, ownerSessionId: "main" });
		const registry = new api.BashToolDetachRegistry();
		const controller = new AbortController();
		const tool = api.createBashToolDefinition(root, { backgroundJobs: { lifecycle, store }, detachRegistry: registry, spawnHook: (context: { env: Record<string, string> }) => ({ ...context, env: { ...context.env, BASH_TEST_ROOT: root } }) });
		const synchronous: unknown[] = [];
		const updates: unknown[] = [];
		let fired = false;
		let result: unknown;
		try {
			result = { ok: true, value: await tool.execute("runner", { command: input.command, timeout: input.timeout }, controller.signal, update => {
				updates.push({ text: update.content.map(entry => entry.text ?? "").join(""), activationExists: existsSync(join(root, "detached-jobs/session/bash_1/activation.json")) });
				if (fired || !update.content.some(entry => entry.text?.includes("ready"))) return;
				fired = true;
				if (input.mode.startsWith("abort")) controller.abort();
				if (input.mode === "detach" || input.mode === "detach-abort" || input.mode === "detach-completed") {
					const detached = registry.detachRunning();
					synchronous.push({ detached, activationExists: existsSync(join(root, "detached-jobs/session/bash_1/activation.json")) });
					if (input.mode === "detach-abort") controller.abort();
				}
			}) };
		} catch (error) { if (!(error instanceof Error)) throw error; result = { ok: false, error: error.message }; }
		const valueResult = result as { value?: { details?: { backgroundJobId?: string } } };
		const jobId = valueResult.value?.details?.backgroundJobId;
		let terminal: string | undefined;
		if (jobId) {
			const deadline = Date.now() + 5000;
			while (Date.now() < deadline) {
				const agent = lifecycle.observe(jobId);
				if (agent && !h.store.isActiveLifecycle(agent.lifecycle)) { terminal = agent.lifecycle; break; }
				await new Promise(resolve => setTimeout(resolve, 25));
			}
			if (terminal === undefined) throw new Error("detached runner did not terminate");
		}
		let payloadSurvived: boolean | undefined;
		if (input.mode === "abort-term-resistant") {
			await new Promise(resolve => setTimeout(resolve, 250));
			payloadSurvived = existsSync(join(root, "abort-survived"));
		}
		output.push({ result, synchronous, updates, terminal, payloadSurvived, hasRunningAfter: registry.hasRunning() });
	}
	return output;
}).catch((error: unknown) => { process.stderr.write(`bash lifecycle oracle failed: ${String(error)}\n`); process.exitCode = 1; });
