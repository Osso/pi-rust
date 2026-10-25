import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const core = dirname(process.argv[2]);
	const load = (name: string) => import(pathToFileURL(join(core, name)).href);
	const { createAgentSession } = await load("sdk.ts");
	const { AuthStorage } = await load("auth-storage.ts");
	const { ModelRegistry } = await load("model-registry.ts");
	const { SessionManager } = await load("session-manager.ts");
	const { SettingsManager } = await load("settings-manager.ts");
	const { DefaultResourceLoader } = await load("resource-loader.ts");
	const { createFauxCore, fauxAssistantMessage } = await load("../../../ai/src/providers/faux.ts");
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const faux = createFauxCore({});
		let captured: unknown;
		faux.setResponses([(_context: unknown, options: unknown) => {
			captured = options;
			return fauxAssistantMessage("ok");
		}]);
		const auth = AuthStorage.inMemory({ faux: { type: "api_key", key: "faux-key", env: { AUTH: "auth", SHARED: "auth" } } });
		const registry = ModelRegistry.inMemory(auth);
		registry.registerProvider("faux", { api: faux.api, apiKey: "faux-key", baseUrl: "https://opencode.ai/v1", streamSimple: faux.streamSimple, models: faux.models });
		const model = { ...faux.getModel(), baseUrl: "https://opencode.ai/v1", headers: { "x-auth": "auth", "x-conflict": "auth" } };
		const settings = SettingsManager.inMemory();
		const resourceLoader = new DefaultResourceLoader({ cwd: request.cwd, agentDir: request.cwd, settingsManager: settings, noExtensions: true });
		const { session } = await createAgentSession({ cwd: request.cwd, agentDir: request.cwd, model, modelRegistry: registry, settingsManager: settings, sessionManager: SessionManager.inMemory(request.cwd), resourceLoader });
		const stream = await session.agent.streamFn(model, { messages: [] }, request.options);
		await stream.result();
		const options = captured as { cwd?: string; headers?: unknown; env?: unknown; apiKey?: string };
		process.stdout.write(`${JSON.stringify({ id: request.id, cwd: options.cwd, headers: options.headers, env: options.env, apiKey: options.apiKey })}\n`);
		await session.dispose();
	}
}
void main().catch((error: unknown) => { console.error(error); process.exit(1); });
