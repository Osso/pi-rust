import { readFileSync } from "node:fs";
import { join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";
type Api = { writeDetachedBashLaunchManifest(path: string, data: unknown): void };
void runOracle(async (module, request) => requireCases(request).map((data, index) => {
	const path = join(String(request.cwd), `manifest-${index}.json`);
	(module as Api).writeDetachedBashLaunchManifest(path, data);
	return readFileSync(path, "utf8");
})).catch((error: unknown) => { process.stderr.write(`bash manifest number oracle failed: ${String(error)}\n`); process.exitCode = 1; });
