import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const load = (name: string) =>
	import(pathToFileURL(`${root}/core/${name}`).href);
const { AgentSession } = await load("agent-session.ts");
const { AgentSessionRuntime } = await load("agent-session-runtime.ts");
const { SessionManager } = await load("session-manager.ts");
const { SettingsManager } = await load("settings-manager.ts");
const { ModelRegistry } = await load("model-registry.ts");
const { AuthStorage } = await load("auth-storage.ts");
const { DefaultResourceLoader } = await load("resource-loader.ts");
const { Agent } = await import(
	pathToFileURL(`${root}/../../agent-core/src/index.ts`).href
);

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as { root: string };
		const agentDir = join(request.root, "agent");
		mkdirSync(agentDir, { recursive: true });
		process.env.PI_CODING_AGENT_DIR = agentDir;
		process.env.PI_CODING_AGENT_STATE_DIR = request.root;
		const authStorage = AuthStorage.inMemory();
		const modelRegistry = ModelRegistry.inMemory(authStorage);
		const settingsManager = SettingsManager.inMemory();
		const resourceLoader = new DefaultResourceLoader({
			cwd: request.root,
			agentDir,
			settingsManager,
		});
		await resourceLoader.reload();
		const session = new AgentSession({
			agent: new Agent({
				initialState: {
					model: modelRegistry.find("anthropic", "claude-sonnet-4-5"),
				},
				streamFn: () => {
					throw new Error("provider must not run");
				},
			}),
			sessionManager: SessionManager.inMemory(request.root),
			settingsManager,
			cwd: request.root,
			agentDir,
			resourceLoader,
			modelRegistry,
			initialActiveToolNames: [],
		});
		await session.bindExtensions({});
		const events: string[] = [];
		session.subscribe((event: { type: string }) => {
			if (event.type === "queue_update") events.push(event.type);
		});
		const services = {
			cwd: request.root,
			agentDir,
			authStorage,
			modelRegistry,
			settingsManager,
			resourceLoader,
			diagnostics: [],
		};
		const runtime = new AgentSessionRuntime(session, services, async () => {
			throw new Error("factory must not run");
		});
		runtime.setBeforeSessionInvalidate(() => {
			throw new Error("fixture stop");
		});
		let error: string | undefined;
		try {
			await runtime.newSession();
		} catch (failure) {
			if (!(failure instanceof Error)) throw failure;
			error = failure.message;
		}
		console.log(
			JSON.stringify({
				ok: true,
				error,
				events,
				sameSession: runtime.session === session,
			}),
		);
		session.dispose();
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
