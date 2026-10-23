// Frozen registry; no runtime/composer modules or live provider requests.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const source = pathToFileURL(process.argv[2]);
	const { ModelRegistry } = await import(source.href);
	const { AuthStorage } = await import(new URL("auth-storage.ts", source).href);
	serve(async (request) => {
		const storage = AuthStorage.inMemory(request.auth ?? {});
		const registry = request.modelsPath
			? ModelRegistry.create(storage, request.modelsPath)
			: ModelRegistry.inMemory(storage);
		const model = registry.find("anthropic", "claude-opus-4-8");
		const snapshot = () => ({
			configured: registry.hasConfiguredAuth(model),
			available: registry.getAvailable().some((item) => item.provider === model.provider && item.id === model.id),
			oauth: registry.isUsingOAuth(model),
		});
		const before = snapshot();
		storage.set("anthropic", { type: "api_key", key: "q15-key" });
		const after = snapshot();
		storage.set("anthropic", { type: "oauth", access: "q15-access", refresh: "q15-refresh", expires: 9999999999999 });
		const oauth = snapshot();
		storage.remove("anthropic");
		return { before, after, oauth, removed: snapshot() };
	});
}
void main();
