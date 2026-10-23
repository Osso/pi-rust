import { runOracle } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
	const processFiles = api.processFileArguments as (paths: string[]) => Promise<unknown>;
	try { return { result: await processFiles(request.paths as string[]) }; }
	catch (error) { return { error: (error as Error).message }; }
}).catch((error: unknown) => { console.error(error); process.exitCode = 1; });
