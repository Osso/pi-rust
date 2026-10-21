import { runOracle, requireCases } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
	if (request.operation === "startupScenario") {
		const { pathToFileURL } = await import("node:url");
		const { dirname, join } = await import("node:path");
		const { VERSION } = await import(pathToFileURL(join(dirname(process.argv[2]), "..", "config.ts")).href);
		const results = [];
		for (const raw of requireCases(request)) {
			const input = raw as {scenario:string;db:string};
			const trace: string[] = [];
			const ready = {version:VERSION,pid:123,executable:"/fake/pi",instanceId:"new",managedBy:"pi",ready:true};
			let resident: unknown = input.scenario === "ready" ? ready : input.scenario === "external" || input.scenario === "replace" ? {...ready,version:"old",instanceId:"old",managedBy:input.scenario === "external" ? "external" : "pi"} : undefined;
			try {
				const result = await (api.ensureSupervisorRunning as (...args: unknown[]) => Promise<unknown>)({controlDbPath:input.db,pollIntervalMs:1,startupTimeoutMs:input.scenario === "timeout" ? 0 : 100,launchInvocation:{command:"/fake/pi",args:["supervisor"],cwd:"/fake",env:{}}},{probe:async()=>{trace.push("probe");return resident;},launch:async()=>{trace.push("launch");if(input.scenario === "launch-error") throw new Error("fake launch error");resident=ready;},terminate:async()=>{trace.push("terminate");resident=undefined;}});
				results.push({result,trace});
			} catch(error) {results.push({error:error instanceof Error ? error.message : String(error),trace});}
		}
		return results;
	}
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
