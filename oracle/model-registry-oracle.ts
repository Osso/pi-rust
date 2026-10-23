// Frozen registry; no runtime/composer modules or live provider requests.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const source = pathToFileURL(process.argv[2]);
	const { ModelRegistry } = await import(source.href);
	const { AuthStorage } = await import(new URL("auth-storage.ts", source).href);
	serve(async (request) => {
		if (request.operation === "oauth_throw") {
			const provider = "q15-oauth-throw";
			const storage = AuthStorage.inMemory({ [provider]: { type: "oauth", access: "access", refresh: "refresh", expires: 9999999999999 } });
			const registry = ModelRegistry.inMemory(storage);
			const rejection = request.throwKind === "error"
				? Object.assign(new TypeError(request.value as string), { code: "E_Q15", detail: request.detail })
				: request.value;
			registry.registerProvider(provider, { oauth: {
				name: "Q15 throw fixture", login: async () => { throw new Error("unused login"); },
				refreshToken: async (credential: unknown) => credential,
				getApiKey: () => { throw rejection; },
			} });
			const snapshot = (error: unknown) => error instanceof Error
				? { kind: "error", value: { name: error.name, message: error.message, ...Object.fromEntries(Object.entries(error).filter(([key]) => key !== "stack")) } }
				: { kind: "value", value: error };
			let key;
			try { key = { ok: true, value: await registry.getApiKeyForProvider(provider) }; }
			catch (error) { key = { ok: false, rejection: snapshot(error) }; }
			let auth;
			const model = { ...registry.find("anthropic", "claude-opus-4-8"), provider };
			try { auth = { ok: true, value: await registry.getApiKeyAndHeaders(model) }; }
			catch (error) { auth = { ok: false, rejection: snapshot(error) }; }
			return { key, auth };
		}
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
