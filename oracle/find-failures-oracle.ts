import * as fs from "node:fs/promises";
import { join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { kind: string; message?: string; paths?: string[]; limit?: number; pathEnv?: string; bin?: string };
type Api = { createFindToolDefinition(cwd: string, options?: unknown): { execute(id: string, args: unknown): Promise<unknown> } };

async function prepareEnvironment(input: Input, cwd: string): Promise<void> {
	if (input.kind === "deleted-cwd") {
		const directory = join(cwd, "source-deleted-cwd");
		await fs.mkdir(directory);
		process.chdir(directory);
		await fs.rmdir(directory);
	}
	if (input.kind === "spawn-failure") {
		await fs.mkdir(input.bin!, { recursive: true });
		const script = `#!${process.execPath}\nconst fs=require('node:fs');if(process.argv[2]==='--version'){fs.unlinkSync(__filename);process.exit(0)}process.exit(1);\n`;
		await fs.writeFile(join(input.bin!, "fd"), script, { mode: 0o755 });
	}
	if (input.pathEnv !== undefined) process.env.PATH = input.pathEnv;
}

async function executeCase(api: Api, cwd: string, input: Input): Promise<unknown> {
	const calls: unknown[] = [];
	const operations = {
		exists(path: string) {
			calls.push(["exists", path]);
			if (input.kind === "exists-error") throw new Error(input.message);
			return input.kind !== "missing";
		},
		glob(pattern: string, path: string, options: { ignore: string[]; limit: number }) {
			calls.push(["glob", pattern, path, options.ignore, String(options.limit)]);
			if (input.kind === "glob-error") throw new Error(input.message);
			return input.paths ?? [];
		},
	};
	const custom = input.kind !== "missing-fd" && input.kind !== "spawn-failure";
	try {
		const tool = api.createFindToolDefinition(cwd, custom ? { operations } : undefined);
		const value = await tool.execute("find-failure", { pattern: "*.x", path: "search", limit: input.limit });
		return { result: { ok: true, value }, calls };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return { result: { ok: false, error: error.message }, calls };
	}
}

void runOracle(async (module, request) => {
	const output = [];
	const cwd = String(request.cwd);
	for (const value of requireCases(request)) {
		const input = value as Input;
		const oldCwd = process.cwd();
		const oldPath = process.env.PATH;
		try {
			await prepareEnvironment(input, cwd);
			output.push(await executeCase(module as Api, cwd, input));
		} finally {
			process.chdir(oldCwd);
			if (oldPath === undefined) delete process.env.PATH;
			else process.env.PATH = oldPath;
		}
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`find failure oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
