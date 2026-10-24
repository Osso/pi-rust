import { existsSync } from "node:fs";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createLocalBashOperations(): {
		exec(command: string, cwd: string, options: unknown): Promise<unknown>;
	};
};
void runOracle(async (api, request) => {
	let result: unknown;
	try {
		result = {
			ok: true,
			value: await (api as Api)
				.createLocalBashOperations()
				.exec(String(request.command), String(request.cwd), {
					onData: () => {},
					detach: {
						signal: new AbortController().signal,
						artifacts: {
							directory: String(request.directory),
							outputPath: String(request.outputPath),
						},
					},
				}),
		};
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		result = { ok: false, error: error.message };
	}
	return { result, artifactExists: existsSync(String(request.directory)) };
}).catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
