import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Input = {
	command: string;
	cwd?: string;
	timeout?: number;
	number?: string;
	artifacts?: boolean;
	missingArtifacts?: boolean;
	preAbort?: boolean;
	abortMs?: number;
	abortOnData?: boolean;
	shellPath?: string;
	env?: Record<string, string>;
	replaceDirectory?: boolean;
	cleanupDirectory?: string;
};
type Api = {
	createLocalBashOperations(options?: { shellPath?: string }): {
		exec(command: string, cwd: string, options: unknown): Promise<unknown>;
	};
};
void runOracle(async (module, request) => {
	const output = [];
	const cwd = String(request.cwd);
	if (request.processCwd === true) process.chdir(cwd);
	let index = 0;
	for (const value of requireCases(request)) {
		const input = value as Input;
		const data: number[] = [];
		const controller = new AbortController();
		if (input.preAbort) controller.abort("reason");
		const abort =
			input.abortMs === undefined
				? undefined
				: setTimeout(() => controller.abort("reason"), input.abortMs);
		const directory = join(cwd, `artifact-${index++}`);
		if (input.artifacts && !input.missingArtifacts) mkdirSync(directory);
		let result: unknown;
		try {
			const ops = (module as Api).createLocalBashOperations({
				shellPath: input.shellPath,
			});
			const command = input.replaceDirectory
				? `rm -rf '${directory}'; printf sentinel > '${directory}'`
				: input.command;
			result = {
				ok: true,
				value: await ops.exec(command, input.cwd ?? cwd, {
					onData: (chunk: Buffer) => {
						data.push(...chunk);
						if (input.abortOnData) controller.abort("reason");
					},
					signal: controller.signal,
					timeout:
						input.number === undefined ? input.timeout : Number(input.number),
					env: input.env,
					detach: input.artifacts
						? {
								artifacts: {
									directory: input.cleanupDirectory ?? directory,
									outputPath: join(directory, "output.log"),
								},
								signal: new AbortController().signal,
							}
						: undefined,
				}),
			};
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		} finally {
			if (abort) clearTimeout(abort);
		}
		output.push({ result, data, artifactExists: existsSync(directory) });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`bash local oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
