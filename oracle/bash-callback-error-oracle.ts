import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Result = { content: unknown[] };
type Api = {
	createBashToolDefinition(
		cwd: string,
		options: unknown,
	): {
		execute(
			id: string,
			args: unknown,
			signal: undefined,
			update: (result: Result) => void,
		): Promise<unknown>;
	};
};
void runOracle(async (api, request) => {
	const records = [];
	for (const value of requireCases(request)) {
		const failure = value as string | null;
		const trace: string[] = [];
		const tool = (api as Api).createBashToolDefinition("/repo", {
			operations: {
				exec: async (
					_command: string,
					_cwd: string,
					options: { onData(data: Buffer): void },
				) => {
					trace.push("before-data");
					options.onData(Buffer.from("payload"));
					trace.push("after-data");
					return { exitCode: 0 };
				},
			},
		});
		let result: unknown;
		try {
			result = {
				ok: true,
				value: await tool.execute(
					"callback",
					{ command: "probe" },
					undefined,
					(update) => {
						trace.push(
							update.content.length ? "data-update" : "initial-update",
						);
						if (update.content.length && failure !== null)
							throw new Error(failure);
					},
				),
			};
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		}
		records.push({ result, trace });
	}
	return records;
}).catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
