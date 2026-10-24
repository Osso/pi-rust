import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Environment = Record<string, string | undefined>;
type Api = {
	createLocalBashOperations(options?: { shellPath?: string }): {
		exec(command: string, cwd: string, options: unknown): Promise<unknown>;
	};
	createBashToolDefinition(
		cwd: string,
		options: unknown,
	): { execute(id: string, args: unknown): Promise<unknown> };
};
void runOracle(async (api, request) => {
	if (request.operation === "envOrder") {
		const bytes: number[] = [];
		await (api as Api)
			.createLocalBashOperations({ shellPath: String(request.shell) })
			.exec("probe", String(request.cwd), {
				env: Object.fromEntries([
					["Z_FIRST", "one"],
					["A_SECOND", "two"],
					["10", "ten"],
					["2", "two"],
				]),
				onData: (chunk: Buffer) => bytes.push(...chunk),
			});
		return bytes;
	}
	if (request.operation === "invalidEnv") {
		try {
			await (api as Api)
				.createLocalBashOperations()
				.exec("true", String(request.cwd), {
					env: Object.fromEntries(
						(Array.isArray(request.keys)
							? (request.keys as string[])
							: [String(request.key)]
						).map((key) => [key, "bad\u0000value"]),
					),
					onData: () => {},
				});
			return { ok: true };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			return { ok: false, error: error.message };
		}
	}
	process.env.BASH_REMOVE = "inherited";
	const records = [];
	for (const mode of requireCases(request) as string[]) {
		if (mode === "custom") {
			let observed: unknown;
			const tool = (api as Api).createBashToolDefinition(String(request.cwd), {
				spawnHook: (context: { env: Environment }) => ({
					...context,
					env: {
						...context.env,
						BASH_REMOVE: undefined,
						BASH_KEEP: String(request.raw),
					},
				}),
				operations: {
					exec: async (
						_command: string,
						_cwd: string,
						options: { env: Environment },
					) => {
						observed = {
							own: Object.hasOwn(options.env, "BASH_REMOVE"),
							value: String(options.env.BASH_REMOVE),
							kept: options.env.BASH_KEEP,
						};
						return { exitCode: 0 };
					},
				},
			});
			await tool.execute("env", { command: "probe" });
			records.push(observed);
		} else {
			const env =
				mode === "default"
					? undefined
					: mode === "empty"
						? {}
						: { BASH_REMOVE: undefined, BASH_KEEP: "kept" };
			const bytes: number[] = [];
			const result = await (api as Api)
				.createLocalBashOperations()
				.exec(
					`printf '%s:%s' "\${BASH_REMOVE-absent}" "\${BASH_KEEP-absent}"`,
					String(request.cwd),
					{ env, onData: (data: Buffer) => bytes.push(...data) },
				);
			records.push({ result, bytes });
		}
	}
	return records;
}).catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
