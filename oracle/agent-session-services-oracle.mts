import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const source = process.argv[2];
const { createAgentSessionServices } = await import(pathToFileURL(source).href);
const { AuthStorage } = await import(pathToFileURL(join(dirname(source), "auth-storage.ts")).href);
const { ModelRegistry } = await import(pathToFileURL(join(dirname(source), "model-registry.ts")).href);
const { SettingsManager } = await import(pathToFileURL(join(dirname(source), "settings-manager.ts")).href);

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const authStorage = AuthStorage.inMemory();
	const services = await createAgentSessionServices({
		cwd: request.cwd,
		agentDir: request.agentDir,
		authStorage,
		modelRegistry: ModelRegistry.create(authStorage, join(request.cwd, "models.json")),
		settingsManager: SettingsManager.inMemory(),
		extensionFlagValues: new Map(request.flags ?? []),
		resourceLoaderOptions: {
			noExtensions: true,
			noSkills: true,
			noPromptTemplates: true,
			noThemes: true,
			noContextFiles: true,
			extensionFactories: [(pi) => {
				pi.registerFlag("verbose", { type: "boolean" });
				pi.registerFlag("label", { type: "string" });
			}],
		},
	});
	const runtime = services.resourceLoader.getExtensions().runtime;
	process.stdout.write(`${JSON.stringify({
		cwd: services.cwd,
		agentDir: services.agentDir,
		diagnostics: services.diagnostics,
		flags: Object.fromEntries(runtime.flagValues),
	})}\n`);
}
