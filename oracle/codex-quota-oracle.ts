import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	serve((request) => ({
		providers: [...api.CODEX_FALLBACK_PROVIDERS],
		results: request.messages.map((message: unknown) => api.isQuotaExhaustionError(message)),
	}));
}
void main();
