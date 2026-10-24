import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Input = {
	agent: unknown;
	timeout?: number;
	registry?: boolean;
	shellPath?: string;
};
type Registry = { hasRunning(): boolean; detachRunning(): boolean };
type Api = {
	BashToolDetachRegistry: new () => Registry;
	createBashToolDefinition(
		cwd: string,
		options: unknown,
	): {
		execute(
			id: string,
			args: unknown,
			signal?: undefined,
			onUpdate?: () => void,
		): Promise<unknown>;
	};
};
void runOracle(async (module, request) => {
	const api = module as Api;
	const output = [];
	process.chdir(String(request.cwd));
	for (const value of requireCases(request)) {
		const input = value as Input;
		const lifecycle = { findBashJobByToolCallId: () => input.agent };
		const registry = new api.BashToolDetachRegistry();
		const initial: unknown[] = [];
		let detachedAtExec = false;
		const operations = input.registry
			? {
					exec: async (
						_command: string,
						_cwd: string,
						options: { detach: { signal: AbortSignal } },
					) => {
						detachedAtExec = options.detach.signal.aborted;
						return { exitCode: 0 };
					},
				}
			: undefined;
		const tool = api.createBashToolDefinition(String(request.cwd), {
			backgroundJobs: { lifecycle, store: {} },
			operations,
			shellPath: input.shellPath,
			detach: input.registry
				? undefined
				: { signal: new AbortController().signal },
			detachRegistry: input.registry ? registry : undefined,
		});
		let result: unknown;
		try {
			result = {
				ok: true,
				value: await tool.execute(
					"restore",
					{ command: "must not launch", timeout: input.timeout },
					undefined,
					input.registry
						? () =>
								initial.push({
									hasRunning: registry.hasRunning(),
									detached: registry.detachRunning(),
								})
						: undefined,
				),
			};
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		}
		output.push(
			input.registry
				? {
						result,
						initial,
						detachedAtExec,
						hasRunningAfter: registry.hasRunning(),
					}
				: result,
		);
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`bash restored oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
