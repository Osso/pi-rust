import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

// Real frozen registry/storage on the same disposable auth/models files as Rust.
async function run(request: any) {
	const core = dirname(process.argv[2]);
	process.env.PI_CODING_AGENT_DIR = request.agentDir;
	process.env.PI_CODING_AGENT_STATE_DIR = join(request.agentDir, "ts-state");
	const [{ ModelRegistry }, { AuthStorage }, { getModel }] = await Promise.all([
		import(pathToFileURL(join(core, "model-registry.ts")).href),
		import(pathToFileURL(join(core, "auth-storage.ts")).href),
		import(pathToFileURL(join(core, "../../../ai/src/compat.ts")).href),
	]);
	const storage = AuthStorage.create(join(request.agentDir, "auth.json"));
	const registry = request.inMemory
		? ModelRegistry.inMemory(storage)
		: ModelRegistry.create(storage, join(request.agentDir, "models.json"));
	if (request.key) storage.setRuntimeApiKey("anthropic", request.key);
	const customModel = registry.find("fixture-path", "private-model");
	const sonnet = getModel("anthropic", "claude-sonnet-4-5");
	return {
		customModel: customModel ?? null,
		customAuth: customModel ? registry.hasConfiguredAuth(customModel) : false,
		customStoredAuth: storage.hasAuth("fixture-path"),
		sonnet,
		anthropicAuth: registry.hasConfiguredAuth(sonnet),
		key: request.key ? await registry.getApiKeyForProvider("anthropic") : null,
	};
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
