import { pathToFileURL } from "node:url";
import { importSource, serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const api = await importSource<any>();
	const capabilities = await import(new URL("./tool-capabilities.ts", pathToFileURL(process.argv[2])).href);
	serve((request) => {
		const options = { ...request.options as object } as any;
		if (request.persisted !== undefined) options.sessionManager = { isPersisted: () => request.persisted };
		api.assertStandaloneWorkerOptions(options);
		return { disabled: [...capabilities.STANDALONE_DISABLED_TOOL_NAMES] };
	});
}
void main();
