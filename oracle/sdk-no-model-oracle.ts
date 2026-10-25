import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const core = dirname(process.argv[2]);
	const load = (name: string) => import(pathToFileURL(join(core, name)).href);
	const { createAgentSession } = await load("sdk.ts");
	const { AgentSessionRuntime } = await load("agent-session-runtime.ts");
	const { AuthStorage } = await load("auth-storage.ts");
	const { ModelRegistry } = await load("model-registry.ts");
	const { SessionManager } = await load("session-manager.ts");
	const { SettingsManager } = await load("settings-manager.ts");
	const { DefaultResourceLoader } = await load("resource-loader.ts");
	const { getPackageDir } = await load("../config.ts");
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const cwd = request.root;
		if (request.operation === "legacy_model") {
			const { createFauxCore } = await load(
				"../../../ai/src/providers/faux.ts",
			);
			const faux = createFauxCore({
				models: [{ id: "preferred" }, { id: "historical" }],
			});
			const registry = ModelRegistry.inMemory(AuthStorage.inMemory());
			registry.registerProvider("faux", {
				api: faux.api,
				apiKey: "fixture",
				baseUrl: "https://faux.invalid",
				streamSimple: faux.streamSimple,
				models: faux.models,
			});
			const settings = SettingsManager.inMemory({
				defaultProvider: "faux",
				defaultModel: "preferred",
			});
			const manager = SessionManager.inMemory(cwd);
			manager.appendModelChange("faux", "historical");
			const resourceLoader = new DefaultResourceLoader({
				cwd,
				agentDir: cwd,
				settingsManager: settings,
			});
			const { session, modelFallbackMessage } = await createAgentSession({
				cwd,
				agentDir: cwd,
				modelRegistry: registry,
				sessionManager: manager,
				settingsManager: settings,
				resourceLoader,
			});
			process.stdout.write(
				`${JSON.stringify({ id: request.id, model: session.model.id, fallback: modelFallbackMessage ?? null })}\n`,
			);
			await session.dispose();
			continue;
		}
		if (request.operation === "resource_order") {
			const settings = SettingsManager.inMemory();
			settings.setSessionSandboxProfile("full-access");
			const seen: unknown[] = [];
			const registry = ModelRegistry.inMemory(AuthStorage.inMemory());
			const { session } = await createAgentSession({
				cwd,
				agentDir: cwd,
				modelRegistry: registry,
				sessionManager: SessionManager.inMemory(cwd),
				settingsManager: settings,
				extensionFactories: [
					() => {
						seen.push(settings.getSessionSandboxProfile() ?? null);
					},
				],
			});
			process.stdout.write(
				`${JSON.stringify({ id: request.id, seen, final: settings.getSessionSandboxProfile() ?? null })}\n`,
			);
			await session.dispose();
			continue;
		}
		const authStorage = AuthStorage.inMemory();
		const modelRegistry = ModelRegistry.inMemory(authStorage);
		const settingsManager = SettingsManager.inMemory();
		const resourceLoader = new DefaultResourceLoader({
			cwd,
			agentDir: cwd,
			settingsManager,
			noExtensions: true,
		});
		const manager = SessionManager.create(cwd, join(cwd, "sessions"));
		manager.setMetadataControlDbPath(join(cwd, "control.db"));
		const { session, modelFallbackMessage } = await createAgentSession({
			cwd,
			agentDir: cwd,
			authStorage,
			modelRegistry,
			settingsManager,
			sessionManager: manager,
			resourceLoader,
		});
		const startup = {
			model: session.model
				? { provider: session.model.provider, id: session.model.id }
				: null,
			message: modelFallbackMessage?.replaceAll(getPackageDir(), "<package>"),
			entries: manager
				.getEntries()
				.map((entry: { type: string }) => entry.type),
		};
		const runtime = new AgentSessionRuntime(
			session,
			{
				cwd,
				agentDir: cwd,
				authStorage,
				modelRegistry,
				settingsManager,
				resourceLoader,
				diagnostics: [],
			},
			async () => {
				throw new Error("factory must not run");
			},
		);
		runtime.setBeforeProcessRestart(() => {
			throw new Error("stop after persistence");
		});
		let error: string | null = null;
		try {
			await runtime.restart({ process: true });
		} catch (failure) {
			error = failure instanceof Error ? failure.message : String(failure);
		}
		process.stdout.write(
			`${JSON.stringify({ id: request.id, startup, persisted: { model: manager.readPersistedSessionSettings()?.model ?? null, thinkingLevel: manager.readPersistedSessionSettings()?.thinkingLevel ?? null }, entries: manager.getEntries().map((entry: { type: string }) => entry.type), error })}\n`,
		);
		await runtime.dispose();
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exit(1);
});
