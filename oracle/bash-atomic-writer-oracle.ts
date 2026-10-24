import { existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Api = {
	writeDetachedBashActivation(path: string, identity: unknown): void;
};
function snapshot(path: string): unknown {
	if (!existsSync(path)) return null;
	const status = statSync(path);
	return status.isDirectory()
		? { directory: true }
		: { bytes: readFileSync(path).toString("hex"), mode: status.mode & 0o777 };
}
void runOracle(async (api, request) => {
	const records = [];
	for (const [index, value] of requireCases(request).entries()) {
		const mode = value as string;
		const directory = join(String(request.root), `source-${index}`);
		mkdirSync(directory);
		const path = join(directory, "activation.json");
		if (mode === "open-directory") mkdirSync(`${path}.tmp`);
		const dirFault = mode.startsWith("dir-");
		process.env.PI_BASH_SPILL_PATH = dirFault ? directory : `${path}.tmp`;
		process.env.PI_BASH_SPILL_EXACT = "1";
		if (mode === "success" || mode === "open-directory")
			delete process.env.PI_BASH_SPILL_FAIL;
		else process.env.PI_BASH_SPILL_FAIL = dirFault ? mode.slice(4) : mode;
		let result: unknown;
		try {
			(api as Api).writeDetachedBashActivation(path, request.identity);
			result = { ok: true };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		}
		delete process.env.PI_BASH_SPILL_FAIL;
		records.push({
			result,
			temporary: snapshot(`${path}.tmp`),
			final: snapshot(path),
		});
	}
	return records;
}).catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
