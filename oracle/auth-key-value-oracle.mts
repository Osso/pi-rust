import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
const { AuthStorage } = await import(pathToFileURL(process.argv[2]).href);
serve(async (request) => {
	const storage = AuthStorage.create(request.path);
	try {
		const value = await storage.getApiKey("anthropic");
		return { ok: true, value: value ?? null };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return { ok: false, name: error.name, message: error.message, constructor: error.constructor.name, typeError: error instanceof TypeError };
	}
});
