import { runOracle, requireCases } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
	if (request.operation === "openResident") {
		const { readdirSync } = await import("node:fs");
		const { basename, join } = await import("node:path");
		return requireCases(request).map((value) => {
			const input = value as {agentDir:string;cwd:string;db:string;role:string};
			const invoke = api[input.role === "supervisor" ? "openSupervisorSession" : "openArchitectSession"] as (...args: string[]) => {getSessionFile():string};
			const manager = invoke(input.agentDir,input.cwd,input.db);
			return {file:basename(manager.getSessionFile()),files:readdirSync(join(input.agentDir,`${input.role}-sessions`)).sort()};
		});
	}
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
