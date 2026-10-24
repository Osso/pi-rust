import { mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = { createLocalBashOperations(): { exec(command: string, cwd: string, options: unknown): Promise<unknown> } };
async function observeExecution(api: Api, mode: string, cwd: string, directory: string): Promise<unknown> {
	let onUncaught: (error: Error & { code?: string; errno?: number; syscall?: string; path?: string }) => void = () => {};
	let timeout: NodeJS.Timeout | undefined;
	const observed = new Promise(resolve => {
		onUncaught = error => resolve({ uncaught: { name: error.name, message: error.message, ...(error.code === undefined ? {} : { code: error.code }), ...(error.errno === undefined ? {} : { errno: error.errno }), ...(error.syscall === undefined ? {} : { syscall: error.syscall }), ...(error.path === undefined ? {} : { path: error.path }) }, constructor: error.constructor.name, string: String(error) });
		process.once("uncaughtException", onUncaught);
		timeout = setTimeout(() => resolve({ oracleTimeout: true }), 3000);
		const command = mode === "poll" ? `rm '${directory}/output.log'; sleep 1.3` : `printf '${process.env.PI_GREP_READ_FAILURE_MARKER}' ${mode === "stderr" ? ">&2" : ""}`;
		void api.createLocalBashOperations().exec(command, cwd, {
			onData: () => {}, detach: mode === "poll" ? { artifacts: { directory, outputPath: join(directory, "output.log") }, signal: new AbortController().signal } : undefined,
		}).then(value => resolve({ ok: true, value }), error => resolve({ ok: false, error: error.message }));
	});
	return observed.finally(() => {
		process.removeListener("uncaughtException", onUncaught);
		if (timeout) clearTimeout(timeout);
	});
}
void runOracle(async (module, request) => {
	const output = [];
	const cwd = String(request.cwd);
	for (const value of requireCases(request)) {
		const mode = String(value);
		const directory = join(cwd, `uncaught-${mode}`);
		if (mode === "poll") mkdirSync(directory);
		try { output.push(await observeExecution(module as Api, mode, cwd, directory)); }
		finally { rmSync(directory, { recursive: true, force: true }); }
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`bash uncaught oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
