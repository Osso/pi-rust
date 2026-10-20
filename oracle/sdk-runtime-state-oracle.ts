import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

// Real pinned runtime objects on the same private auth/models files as Rust.
async function run(request: any) {
	const core = dirname(process.argv[2]);
	process.env.PI_CODING_AGENT_DIR = request.agentDir;
	process.env.PI_CODING_AGENT_STATE_DIR = join(request.agentDir, "ts-state");
	const { ModelRuntime } = await import(
		pathToFileURL(join(core, "model-runtime.ts")).href
	);
	const { getModel } = await import(
		pathToFileURL(join(core, "../../../ai/src/compat.ts")).href
	);
	const runtime = await ModelRuntime.create({
		authPath: join(request.agentDir, "auth.json"),
		modelsPath: join(request.agentDir, "models.json"),
		allowModelNetwork: false,
	});
	if (request.key) await runtime.setRuntimeApiKey("anthropic", request.key);
	const resolved = request.key ? await runtime.getAuth("anthropic") : undefined;
	return {
		customModel: runtime.getModel("fixture-path", "private-model"),
		customAuth: runtime.hasConfiguredAuth("fixture-path"),
		sonnet: getModel("anthropic", "claude-sonnet-4-5"),
		anthropicAuth: runtime.hasConfiguredAuth("anthropic"),
		key: resolved?.auth.apiKey ?? null,
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
