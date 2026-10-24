import { mkdirSync, rmSync, existsSync } from "node:fs";
import { join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Process = { artifacts?: unknown; child: { pid?: number }; command: string; cwd: string; exit: Promise<number | null>; pid?: number; startedAt: number; timeoutHandle?: NodeJS.Timeout };
type Input = { mode: string; artifacts?: boolean; timeout?: number };
type Api = { createLocalBashOperations(): { exec(command: string, cwd: string, options: unknown): Promise<unknown> } };
void runOracle(async (module, request) => {
	const cwd = String(request.cwd);
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as Input;
		const directory = join(cwd, `adopt-${input.mode}`);
		if (input.artifacts) mkdirSync(directory);
		const controller = new AbortController();
		controller.abort();
		const calls: unknown[] = [];
		let result: unknown;
		try {
			result = { ok: true, value: await (module as Api).createLocalBashOperations().exec("sleep .1; exit 7", cwd, {
				onData: () => {}, timeout: input.timeout,
				detach: { signal: controller.signal, artifacts: input.artifacts ? { directory, outputPath: join(directory, "output.log") } : undefined,
					adopt: (process: Process) => {
						calls.push({ command: process.command, cwd: process.cwd, pidMatches: typeof process.pid === "number" && process.child.pid === process.pid, startedAtValid: Number.isFinite(process.startedAt), hasTimeout: process.timeoutHandle !== undefined, hasArtifacts: process.artifacts !== undefined });
						if (input.mode === "reject") return Promise.reject(new Error("adopt failed"));
						if (input.mode === "clear") clearTimeout(process.timeoutHandle);
						const detached = { jobId: input.mode, message: "adopted" };
						return input.mode === "exit" || input.mode === "clear" ? process.exit.then(() => detached) : detached;
					},
				},
			}) };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		}
		output.push({ result, calls, artifactExists: existsSync(directory) });
		rmSync(directory, { recursive: true, force: true });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`bash adopt oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
