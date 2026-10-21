import { runOracle, requireCases } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
	if (request.operation === "restartPending") {
		const { pathToFileURL } = await import("node:url");
		const { dirname, join } = await import("node:path");
		const { SessionManager } = await import(pathToFileURL(join(dirname(process.argv[2]), "session-manager.ts")).href);
		return requireCases(request).map((messages) => {
			const manager = SessionManager.inMemory("/fixture");
			for (const message of messages as unknown[]) manager.appendMessage(message);
			(api.resolveRestartInterruptedToolCalls as (manager: unknown) => void)(manager);
			return manager.buildSessionContext().messages.filter((message: { role: string }) => message.role === "toolResult").map((message: Record<string, unknown>) => ({ ...message, timestamp: 0 }));
		});
	}
	const invoke = api[request.operation] as (...args: unknown[]) => unknown;
	return Promise.all(requireCases(request).map(async (value) => {
		const args = value as unknown[];
		return (await invoke(...args)) ?? null;
	}));
}).catch((error: unknown) => {
	process.stderr.write(`${String(error)}\n`);
	process.exitCode = 1;
});
