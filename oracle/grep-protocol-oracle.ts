import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { context?: number; limit?: number; content?: string; readError?: boolean; isDirectory?: boolean };
type Api = { createGrepToolDefinition(cwd: string, options: unknown): { execute(id: string, args: unknown): Promise<unknown> } };
type Tool = ReturnType<Api["createGrepToolDefinition"]>;
function observeExecution(tool: Tool, input: Input): Promise<unknown> {
	let onUncaught: (error: unknown) => void = () => {};
	const observed = new Promise(resolve => {
		onUncaught = error => {
			if (!(error instanceof Error)) throw error;
			const native = error as Error & { code?: string };
			resolve({ uncaught: { name: error.name, message: error.message, ...(native.code === undefined ? {} : { code: native.code }) }, constructor: error.constructor.name, string: String(error) });
		};
		process.once("uncaughtException", onUncaught);
		process.once("unhandledRejection", onUncaught);
		void tool.execute("grep-protocol", { pattern: "x", path: "search", context: input.context, limit: input.limit })
			.then(value => resolve({ ok: true, value }), error => resolve({ ok: false, error: error.message }));
	});
	return observed.finally(() => {
		process.removeListener("uncaughtException", onUncaught);
		process.removeListener("unhandledRejection", onUncaught);
	});
}
void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as Input;
		process.env.PI_FAKE_RG_PROTOCOL = JSON.stringify(value);
		const calls: unknown[] = [];
		const tool = (module as Api).createGrepToolDefinition("/repo", {
			operations: {
				isDirectory(path: string) { calls.push(["isDirectory", path]); return input.isDirectory ?? true; },
				readFile(path: string) {
					calls.push(["readFile", path]);
					if (input.readError) throw new Error("scripted read failure");
					return input.content ?? "one\ntwo\nthree\nfour\nfive";
				},
			},
		});
		output.push({ result: await observeExecution(tool, input), calls });
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`grep protocol oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
