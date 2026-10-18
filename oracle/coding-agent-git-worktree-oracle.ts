import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type ExecResult = {
	stdout?: string;
	stderr?: string;
	reject?: { message: string; stderr?: string; stdout?: string };
};

type Case = { name: string; cwd: string; worktreeName: string; results: ExecResult[] };

type GitExec = (
	args: string[],
	options: { cwd: string },
) => Promise<{ stdout: string; stderr: string }>;

void runOracle(async (api, request) => {
	const resolveWorktree = api.resolveWorktree as (
		name: string,
		options: { cwd: string; exec: GitExec },
	) => Promise<string>;

	const results: unknown[] = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		const remaining = [...input.results];
		const calls: Array<{ args: string[]; cwd: string }> = [];
		const exec: GitExec = async (args, options) => {
			calls.push({ args, cwd: options.cwd });
			const result = remaining.shift();
			if (result === undefined) throw new Error(`Unexpected git call: ${args.join(" ")}`);
			if (result.reject) {
				const error = new Error(result.reject.message) as Error & { stderr?: string; stdout?: string };
				if (result.reject.stderr !== undefined) error.stderr = result.reject.stderr;
				if (result.reject.stdout !== undefined) error.stdout = result.reject.stdout;
				throw error;
			}
			return { stdout: result.stdout ?? "", stderr: result.stderr ?? "" };
		};

		try {
			const worktree = await resolveWorktree(input.worktreeName, { cwd: input.cwd, exec });
			results.push({ name: input.name, ok: true, value: worktree, calls });
		} catch (error) {
			results.push({ name: input.name, ok: false, message: (error as Error).message, calls });
		}
	}
	return { results };
}).catch((error: unknown) => {
	process.stderr.write(`git-worktree oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
