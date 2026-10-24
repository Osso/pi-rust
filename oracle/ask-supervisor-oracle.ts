import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type ToolApi = {
	createAskSupervisorToolDefinition(): {
		execute(
			id: string,
			params: unknown,
			signal: AbortSignal | undefined,
			update: (value: unknown) => void,
			context: unknown,
		): Promise<unknown>;
	};
};
type SessionApi = {
	SessionManager: { inMemory(cwd: string, options: { id: string }): unknown };
};
type ConsoleApi = {
	probeResidentConsole(input: {
		socketPath: string;
		service: string;
	}): Promise<unknown>;
};

async function loadSibling(relative: string): Promise<unknown> {
	const source = process.argv[2];
	if (!source)
		throw new Error("ask supervisor oracle requires a source module");
	return import(pathToFileURL(join(dirname(source), relative)).href);
}

void runOracle(async (module, request) => {
	if (request.operation === "probe") {
		const transport = (await loadSibling(
			"../resident-console-transport.ts",
		)) as ConsoleApi;
		const config = (await loadSibling("../../config.ts")) as {
			VERSION: string;
		};
		return {
			version: config.VERSION,
			snapshot: await transport.probeResidentConsole({
				socketPath: `${String(request.controlDbPath)}.supervisor-console.sock`,
				service: "supervisor",
			}),
		};
	}
	const sessions = (await loadSibling("../session-manager.ts")) as SessionApi;
	const updates: unknown[] = [];
	const context = {
		cwd: request.cwd,
		controlDbPath: request.controlDbPath,
		sessionManager: sessions.SessionManager.inMemory(request.cwd as string, {
			id: request.sender as string,
		}),
	};
	try {
		const result = await (module as unknown as ToolApi)
			.createAskSupervisorToolDefinition()
			.execute(
				"advisory",
				request.params,
				request.aborted ? AbortSignal.abort("ignored") : undefined,
				(value) => updates.push(value),
				context,
			);
		return { ok: true, result, updates };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return { ok: false, error: error.message, updates };
	}
}).catch((error: unknown) => {
	process.stderr.write(`ask supervisor oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
