import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Environment = Record<string, string | undefined>;
type Api = {
	createLocalBashOperations(): {
		exec(command: string, cwd: string, options: unknown): Promise<unknown>;
	};
	createBashToolDefinition(
		cwd: string,
		options: unknown,
	): { execute(id: string, args: unknown): Promise<unknown> };
};
void runOracle(async (api, request) => {
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
