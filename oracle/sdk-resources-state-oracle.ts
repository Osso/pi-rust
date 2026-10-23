import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { createRequire } from "node:module";
import vm from "node:vm";

// Execute the pinned 04/07/08 programs with real pinned loaders and sessions.
// Only filesystem roots/model selection/console formatting are fixture inputs.
async function run(request: any) {
	const sourcePath = process.argv[2];
	const core = resolve(dirname(sourcePath), "../../src/core");
	process.env.PI_CODING_AGENT_DIR = request.agentDir;
	process.env.PI_CODING_AGENT_STATE_DIR = join(request.agentDir, "ts-state");
	const load = (name: string) => import(pathToFileURL(join(core, name)).href);
	const { createAgentSession } = await load("sdk.ts");
	const { DefaultResourceLoader } = await load("resource-loader.ts");
	const { SettingsManager } = await load("settings-manager.ts");
	const { SessionManager } = await load("session-manager.ts");
	const { ModelRegistry } = await load("model-registry.ts");
	const { AuthStorage } = await load("auth-storage.ts");
	const { createSyntheticSourceInfo } = await load("source-info.ts");
	const settingsManager = SettingsManager.inMemory({});
	const storage = AuthStorage.inMemory();
	const registry = ModelRegistry.create(storage, join(request.agentDir, "models.json"));
	let observed: any;
	const sdk = {
		SessionManager,
		createSyntheticSourceInfo,
		getAgentDir: () => request.agentDir,
		DefaultResourceLoader: class extends DefaultResourceLoader {
			constructor(options: any) {
				super({
					...options,
					cwd: request.cwd,
					agentDir: request.agentDir,
					settingsManager,
				});
			}
		},
		async createAgentSession(options: any) {
			const result = await createAgentSession({
				...options,
				cwd: request.cwd,
				agentDir: request.agentDir,
				settingsManager,
				authStorage: storage,
				modelRegistry: registry,
				model: registry.find("anthropic", "claude-sonnet-4-5"),
			});
			const session = result.session;
			observed = {
				systemPrompt: session.systemPrompt,
				skills: session.resourceLoader.getSkills().skills,
				agents: session.resourceLoader.getAgentsFiles().agentsFiles,
				prompts: session.promptTemplates,
			};
			return result;
		},
	};
	const ts = createRequire(
		resolve(dirname(sourcePath), "../../../..", "package.json"),
	)("typescript");
	const javascript = ts.transpileModule(readFileSync(sourcePath, "utf8"), {
		compilerOptions: {
			target: ts.ScriptTarget.ES2022,
			module: ts.ModuleKind.CommonJS,
		},
	}).outputText;
	const context = vm.createContext({
		process: { cwd: () => request.cwd },
		console: { log() {}, warn() {} },
		require: (name: string) => {
			if (name === "@earendil-works/pi-coding-agent") return sdk;
			throw new Error(`Unexpected import ${name}`);
		},
		exports: {},
	});
	await vm.runInContext(`(async()=>{${javascript}})()`, context);
	return observed;
}
async function main() {
	let pending = "";
	for await (const chunk of process.stdin) {
		pending += chunk;
		let end: number;
		while ((end = pending.indexOf("\n")) !== -1) {
			const request = JSON.parse(pending.slice(0, end));
			pending = pending.slice(end + 1);
			console.log(JSON.stringify(await run(request)));
		}
	}
}
main().catch((error) => {
	console.error(error);
	process.exit(1);
});
