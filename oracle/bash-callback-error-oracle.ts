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
async function observeStreamFailure(
	api: Api,
	failure: string,
): Promise<unknown> {
	const trace: string[] = [];
	const tool = api.createBashToolDefinition(process.cwd(), {});
	return await new Promise((resolve) => {
		const onError = (error: Error) => {
			clearTimeout(timer);
			resolve({
				uncaught: { name: error.name, message: error.message },
				trace,
			});
		};
		const timer = setTimeout(() => {
			process.removeListener("uncaughtException", onError);
			resolve({ timedOut: true });
		}, 3000);
		process.once("uncaughtException", onError);
		void tool
			.execute(
				"callback",
				{ command: "printf payload" },
				undefined,
				(update) => {
					trace.push(update.content.length ? "data-update" : "initial-update");
					if (update.content.length) throw new Error(failure);
				},
			)
			.then(
				(value) => {
					clearTimeout(timer);
					process.removeListener("uncaughtException", onError);
					resolve({ returned: value, trace });
				},
				(error) => {
					clearTimeout(timer);
					process.removeListener("uncaughtException", onError);
					resolve({ rejected: String(error), trace });
				},
			);
	});
}
void runOracle(async (api, request) => {
	const records = [];
	for (const value of requireCases(request)) {
		const failure = value as string | null;
		if (request.operation === "stream") {
			records.push(await observeStreamFailure(api as Api, failure as string));
			continue;
		}
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
