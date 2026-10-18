import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

async function main(): Promise<void> {
	const api = await import(process.argv[2] ?? "");
	const backend = api as unknown as {
		resolveBwrapSandboxProfile(profile: string): unknown;
		createBwrapRunnerEnvironment(hostEnv: Record<string, string | undefined>, pythonPath?: string): unknown;
		buildBwrapInvocation(options: unknown): unknown;
	};
	await runOracle((_module, request) => {
		const cases = requireCases(request);
		switch (request.operation) {
			case "profile":
				return cases.map((value) => backend.resolveBwrapSandboxProfile(String(value)));
			case "runner-env":
				return cases.map((value) => {
					const input = value as { hostEnv: Record<string, string>; pythonPath?: string };
					return backend.createBwrapRunnerEnvironment(input.hostEnv, input.pythonPath);
				});
			case "invocation":
				return cases.map((value) => backend.buildBwrapInvocation(value));
			default:
				throw new Error(`unknown bwrap operation ${request.operation}`);
		}
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`coding-agent bwrap oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
