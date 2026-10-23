import { rmdirSync } from "node:fs";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type RuntimeBuilder = { _buildRuntime(options: object): void };

void runOracle((api, request) => {
	const savedCwd = process.cwd();
	const retained = new Map();
	const session = {
		settingsManager: {
			getImageAutoResize: () => true,
			getShellCommandPrefix: () => undefined,
			getShellPath: () => undefined,
		},
		_cwd: request.cwd,
		_baseToolDefinitions: retained,
	};
	try {
		process.chdir(String(request.deletedCwd));
		rmdirSync(String(request.deletedCwd));
		const constructor = api.AgentSession as { prototype: RuntimeBuilder };
		constructor.prototype._buildRuntime.call(session, {});
		return { message: "unexpected success" };
	} catch (error) {
		return { message: (error as Error).message, retained: session._baseToolDefinitions === retained };
	} finally { process.chdir(savedCwd); }
}).catch((error: unknown) => { console.error(error); process.exitCode = 1; });
