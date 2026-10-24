import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { chunks: number[][]; error?: string; abort?: boolean };
void runOracle(async (_module, request) => {
	const executor = await import(pathToFileURL(join(dirname(process.argv[2]), "../bash-executor.ts")).href);
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as Input;
		const chunks: string[] = [];
		const calls: unknown[] = [];
		const controller = new AbortController();
		let result: unknown;
		try {
			result = { ok: true, value: await executor.executeBashWithOperations("command", "/cwd", {
				exec: async (command: string, cwd: string, options: { onData(data: Buffer): void; env?: unknown; timeout?: unknown; detach?: unknown }) => {
					calls.push({ command, cwd, hasEnv: options.env !== undefined, hasTimeout: options.timeout !== undefined, hasDetach: options.detach !== undefined });
					for (const bytes of input.chunks) options.onData(Buffer.from(bytes));
					if (input.abort) controller.abort("reason");
					if (input.error) throw new Error(input.error);
					return { exitCode: 0 };
				},
			}, { signal: controller.signal, onChunk: (text: string) => chunks.push(text) }) };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		}
		output.push({ result, chunks, calls });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`bash executor stream oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
