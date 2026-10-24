import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Options = { onData(data: Buffer): void };
type Api = {
	createBashToolDefinition(
		cwd: string,
		options: unknown,
	): {
		execute(
			id: string,
			args: unknown,
			signal: undefined,
			update: () => void,
		): Promise<unknown>;
	};
};
void runOracle(async (api, request) => {
	const executor = await import(
		pathToFileURL(join(dirname(process.argv[2]), "../bash-executor.ts")).href
	);
	const records = [];
	for (const value of requireCases(request)) {
		const input = value as { kind: string; failure: string; abort?: boolean };
		const trace: string[] = [];
		const controller = new AbortController();
		const operations = {
			exec: async (_command: string, _cwd: string, options: Options) => {
				trace.push("before-data");
				options.onData(Buffer.from("payload"));
				trace.push("after-data");
				return { exitCode: 0 };
			},
		};
		let result: unknown;
		try {
			if (input.kind === "hook") {
				const tool = (api as Api).createBashToolDefinition("/repo", {
					operations,
					spawnHook: () => {
						trace.push("hook");
						throw new Error(input.failure);
					},
				});
				result = {
					ok: true,
					value: await tool.execute(
						"hook",
						{ command: "probe" },
						undefined,
						() => trace.push("update"),
					),
				};
			} else {
				result = {
					ok: true,
					value: await executor.executeBashWithOperations(
						"probe",
						"/repo",
						operations,
						{
							signal: controller.signal,
							onChunk: (text: string) => {
								trace.push(`chunk:${text}`);
								if (input.abort) controller.abort("reason");
								throw new Error(input.failure);
							},
						},
					),
				};
			}
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
