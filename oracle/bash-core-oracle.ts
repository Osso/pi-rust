import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";
type Input = {
	command: string; prefix?: string; timeout?: number; number?: string; exitCode?: number | null;
	chunks?: number[][]; error?: string; recordEnv?: boolean;
	hook?: { command?: string; cwd?: string; env?: Record<string, string> };
	detached?: { jobId: string; message: string; logPath?: string };
};
type Api = { createBashToolDefinition(cwd: string, options: unknown): { execute(id: string, args: unknown, signal: undefined, onUpdate: (value: unknown) => void): Promise<unknown> } };
const environmentNames = ["PI_SESSION_ID", "PI_SESSION_FILE", "PI_PROVIDER", "PI_MODEL", "PI_REASONING_LEVEL"];
void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as Input;
		for (const name of environmentNames) process.env[name] = `inherited-${name}`;
		const calls: unknown[] = [];
		const updates: unknown[] = [];
		const operations = {
			async exec(command: string, cwd: string, options: { onData(data: Buffer): void; timeout?: number; env: Record<string, string> }) {
				calls.push({ command, cwd, timeout: String(options.timeout), ...(input.recordEnv ? { env: Object.fromEntries(environmentNames.filter(name => options.env[name] !== undefined).map(name => [name, options.env[name]])) } : {}) });
				for (const chunk of input.chunks ?? []) options.onData(Buffer.from(chunk));
				if (input.error !== undefined) throw new Error(input.error);
				return { exitCode: input.exitCode === undefined ? 0 : input.exitCode, detached: input.detached };
			},
		};
		const hook = input.hook;
		const tool = (module as Api).createBashToolDefinition("/repo", {
			operations, commandPrefix: input.prefix,
			spawnHook: hook ? (context: { command: string; cwd: string; env: Record<string, string> }) => ({ command: hook.command ?? context.command, cwd: hook.cwd ?? context.cwd, env: { ...context.env, ...hook.env } }) : undefined,
		});
		const timeout = input.number === undefined ? input.timeout : Number(input.number);
		let result: unknown;
		try { result = { ok: true, value: await tool.execute("bash-core", { command: input.command, timeout }, undefined, update => updates.push(update)) }; }
		catch (error) { if (!(error instanceof Error)) throw error; result = { ok: false, error: error.message }; }
		output.push({ result, calls, updates });
	}
	return output;
}).catch((error: unknown) => { process.stderr.write(`bash core oracle failed: ${String(error)}\n`); process.exitCode = 1; });
