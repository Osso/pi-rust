import { runOracle, requireCases } from "./coding-agent-utils-oracle-support.ts";

type StartupApi = { shouldRunFirstTimeSetup(path: string): boolean };
type SetupCase = { experimental: string | null; agentDir: string | null; settingsPath: string };

void runOracle((module, request) => {
	const api = module as unknown as StartupApi;
	return requireCases(request).map((value) => {
		const input = value as SetupCase;
		if (input.experimental === null) delete process.env.PI_EXPERIMENTAL;
		else process.env.PI_EXPERIMENTAL = input.experimental;
		if (input.agentDir === null) delete process.env.PI_CODING_AGENT_DIR;
		else process.env.PI_CODING_AGENT_DIR = input.agentDir;
		return api.shouldRunFirstTimeSetup(input.settingsPath);
	});
}).catch((error: unknown) => {
	console.error("startup-ui oracle failed:", error);
	process.exitCode = 1;
});
