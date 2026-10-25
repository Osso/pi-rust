import { readFileSync } from "node:fs";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type StorageApi = {
	AuthStorage: { create(path: string): {
		getApiKey(id: string, options: { throwOnRefreshFailure: boolean }): Promise<string | undefined>;
		get(id: string): unknown;
	} };
};

void runOracle(async (module, request) => {
	const api = module as unknown as StorageApi;
	const savedFetch = globalThis.fetch;
	const savedNow = Date.now;
	Date.now = () => 1750000000000;
	globalThis.fetch = async () => new Response(JSON.stringify(request.tokenResponse), {
		status: 200, headers: { "content-type": "application/json" },
	});
	try {
		const path = String(request.authPath);
		const storage = api.AuthStorage.create(path);
		const key = await storage.getApiKey("openai-codex", { throwOnRefreshFailure: true });
		return { key, credential: JSON.stringify(storage.get("openai-codex")), file: readFileSync(path, "utf8") };
	} finally {
		globalThis.fetch = savedFetch;
		Date.now = savedNow;
	}
}).catch((error: unknown) => {
	console.error("Codex credential persistence oracle failed:", error);
	process.exitCode = 1;
});
