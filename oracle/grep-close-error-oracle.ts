import * as fs from "node:fs/promises";
import { join } from "node:path";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";
type Api = { createGrepToolDefinition(cwd: string, options: unknown): { execute(id: string, args: unknown): Promise<unknown> } };
void runOracle(async (module, request) => {
	const oldCwd = process.cwd();
	const deleted = join(String(request.cwd), "source-deleted-cwd");
	await fs.mkdir(deleted);
	process.chdir(deleted);
	await fs.rmdir(deleted);
	let settled = false;
	let onUnhandled: (error: unknown) => void = () => {};
	try {
		return await new Promise(resolve => {
			onUnhandled = error => {
				if (!(error instanceof Error)) throw error;
				const native = error as Error & { code?: string; errno?: number; syscall?: string };
				resolve({ kind: "unhandledRejection", error: { name: error.name, message: error.message, code: native.code, errno: native.errno, syscall: native.syscall }, settled });
			};
			process.once("unhandledRejection", onUnhandled);
			const tool = (module as Api).createGrepToolDefinition(String(request.cwd), {
				operations: { isDirectory: () => true, readFile: () => "unused" },
			});
			void tool.execute("grep-close-error", { pattern: "x" }).then(
				value => { settled = true; resolve({ kind: "tool", value }); },
				error => { settled = true; resolve({ kind: "tool", message: error.message }); },
			);
		});
	} finally {
		process.removeListener("unhandledRejection", onUnhandled);
		process.chdir(oldCwd);
	}
}).catch((error: unknown) => { process.stderr.write(`grep close error oracle failed: ${String(error)}\n`); process.exitCode = 1; });
