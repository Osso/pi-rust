import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { pathToFileURL } from "node:url";

// Real pinned AgentSession and AgentSessionRuntime; no replacement abort/teardown.
async function main() {
	const dir = mkdtempSync(join(tmpdir(), "sdk-runtime-order-"));
	process.env.PI_CODING_AGENT_DIR = dir;
	process.env.PI_CODING_AGENT_STATE_DIR = dir;
	const core = dirname(process.argv[2]);
	const load = (name: string) => import(pathToFileURL(join(core, name)).href);
	const { AgentSession } = await load("agent-session.ts");
	const { AgentSessionRuntime } = await load("agent-session-runtime.ts");
	const { SessionManager } = await load("session-manager.ts");
	const { SettingsManager } = await load("settings-manager.ts");
	const { ModelRuntime } = await load("model-runtime.ts");
	const { AuthStorage } = await load("auth-storage.ts");
	const { DefaultResourceLoader } = await load("resource-loader.ts");
	const { Agent } = await import(
		pathToFileURL(join(core, "../../../agent/src/agent.ts")).href
	);
	try {
		const modelRuntime = await ModelRuntime.create({
			credentials: AuthStorage.inMemory(),
			modelsPath: join(dir, "models.json"),
			allowModelNetwork: false,
		});
		const settingsManager = SettingsManager.inMemory();
		let starts = 0;
		const resourceLoader = new DefaultResourceLoader({
			cwd: dir,
			agentDir: dir,
			settingsManager,
			extensionFactories: [
				(api: any) => {
					api.on("session_start", () => {
						starts++;
					});
				},
			],
		});
		await resourceLoader.reload();
		const session = new AgentSession({
			agent: new Agent({
				streamFn: () => {
					throw new Error("provider must not run");
				},
			}),
			sessionManager: SessionManager.inMemory(dir),
			settingsManager,
			cwd: dir,
			agentDir: dir,
			resourceLoader,
			modelRuntime,
			initialActiveToolNames: [],
		});
		await session.bindExtensions({});
		const bindStarts = starts;
		let disposed = false,
			replacements = 0;
		const dispose = session.dispose.bind(session);
		session.dispose = () => {
			disposed = true;
			dispose();
		};
		const services = {
			cwd: dir,
			agentDir: dir,
			modelRuntime,
			settingsManager,
			resourceLoader,
			diagnostics: [],
		};
		const runtime = new AgentSessionRuntime(session, services, async () => {
			replacements++;
			throw new Error("unexpected replacement");
		});
		const unsubscribe = session.subscribe((event: any) => {
			if (event.type === "queue_update") throw new Error("queue output failed");
		});
		let error;
		try {
			await runtime.newSession();
		} catch (failure: any) {
			error = failure.message;
		}
		console.log(
			JSON.stringify({
				error,
				disposed,
				replacements,
				sameSession: runtime.session === session,
				bindStarts,
			}),
		);
		unsubscribe();
		session.dispose();
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
}
main().catch((error) => {
	console.error(error);
	process.exit(1);
});
