import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case =
	| { kind: "expand"; path: string }
	| { kind: "toCwd"; path: string; cwd: string }
	| { kind: "read"; path: string; cwd: string }
	| { kind: "readAsync"; path: string; cwd: string };

type Api = {
	expandPath(path: string): string;
	resolveToCwd(path: string, cwd: string): string;
	resolveReadPath(path: string, cwd: string): string;
	resolveReadPathAsync(path: string, cwd: string): Promise<string>;
};

void runOracle(async (module, request) => {
	const api = module as unknown as Api;
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		try {
			const resolved =
				input.kind === "expand"
					? api.expandPath(input.path)
					: input.kind === "toCwd"
						? api.resolveToCwd(input.path, input.cwd)
						: input.kind === "read"
							? api.resolveReadPath(input.path, input.cwd)
							: await api.resolveReadPathAsync(input.path, input.cwd);
			results.push({ ok: true, value: resolved });
		} catch (error) {
			results.push({ ok: false, error: error instanceof Error ? `${error.name}: ${error.message}` : String(error) });
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent path-utils oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
