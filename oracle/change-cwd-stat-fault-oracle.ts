import fs from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createChangeWorkingDirectoryToolDefinition(): {
		execute(
			id: string,
			params: unknown,
			signal: undefined,
			update: undefined,
			context: unknown,
		): Promise<unknown>;
	};
};

void runOracle(async (module, request) => {
	const relocations: string[] = [];
	const context = {
		cwd: request.cwd,
		relocateAfterToolResult: (_id: string, cwd: string) =>
			relocations.push(cwd),
	};
	const exists = fs.existsSync;
	// Test-only dependency wrapper bypasses exists, so the actual frozen statSync throws.
	fs.existsSync = (path) => path === request.path || exists(path);
	syncBuiltinESMExports();
	try {
		await (module as unknown as Api)
			.createChangeWorkingDirectoryToolDefinition()
			.execute(
				"stat-fault",
				{ path: request.path },
				undefined,
				undefined,
				context,
			);
		return { ok: true, relocations };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		const native = error as NodeJS.ErrnoException;
		return {
			ok: false,
			error: {
				name: native.name,
				message: native.message,
				code: native.code,
				errno: native.errno,
				syscall: native.syscall,
				path: native.path,
			},
			relocations,
		};
	} finally {
		fs.existsSync = exists;
		syncBuiltinESMExports();
	}
}).catch((error: unknown) => {
	process.stderr.write(`change-cwd stat oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
