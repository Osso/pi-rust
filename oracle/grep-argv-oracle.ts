import * as fs from "node:fs/promises";
import { join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";
type Input = { kind: string; directory: string; args: Record<string, unknown>; filePath: string; isDirectory?: boolean; local?: boolean; message?: string };
type Api = { createGrepToolDefinition(cwd: string, options?: unknown): { execute(id: string, args: unknown): Promise<unknown> } };
async function executeCase(api: Api, cwd: string, input: Input, script: string): Promise<unknown> {
	const calls: unknown[] = [];
	const bin = join(input.directory, "bin");
	const argvFile = join(input.directory, "argv.json");
	await fs.mkdir(bin, { recursive: true });
	await fs.rm(argvFile, { force: true });
	if (input.kind !== "missing") {
		await fs.writeFile(join(bin, "rg"), `#!${process.execPath}\n${script}`, { mode: 0o755 });
		await fs.chmod(join(bin, "rg"), 0o755);
	}
	process.env.PATH = bin;
	process.env.PI_FAKE_RG_ARGV = JSON.stringify({ ...input, argvFile });
	const operations = {
		isDirectory(path: string) { calls.push(["isDirectory", path]); if (input.kind === "stat-error") throw new Error(input.message); return input.isDirectory ?? true; },
		readFile(path: string) { calls.push(["readFile", path]); return "one\ntwo\nthree"; },
	};
	let result: unknown;
	try { result = { ok: true, value: await api.createGrepToolDefinition(cwd, input.local ? undefined : { operations }).execute("grep-argv", input.args) }; }
	catch (error) { if (!(error instanceof Error)) throw error; result = { ok: false, error: error.message }; }
	let argv: unknown = null;
	try { argv = JSON.parse(await fs.readFile(argvFile, "utf8")); }
	catch (error) { if ((error as { code?: string }).code !== "ENOENT") throw error; }
	return { result, calls, argv };
}
void runOracle(async (module, request) => {
	const output = [];
	const oldPath = process.env.PATH;
	try {
		for (const input of requireCases(request)) output.push(await executeCase(module as Api, String(request.cwd), input as Input, String(request.script)));
	} finally { if (oldPath === undefined) delete process.env.PATH; else process.env.PATH = oldPath; }
	return output;
}).catch((error: unknown) => { process.stderr.write(`grep argv oracle failed: ${String(error)}\n`); process.exitCode = 1; });
