import * as fs from "node:fs/promises";
import { join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createLsToolDefinition(cwd: string, options: unknown): {
		execute(id: string, args: unknown): Promise<unknown>;
	};
};

async function createFixture(cwd: string, scenario: string): Promise<string> {
	const target = join(cwd, "target");
	await fs.mkdir(cwd);
	if (scenario === "missing") return target;
	if (scenario === "file") {
		await fs.writeFile(target, "file");
		return target;
	}
	await fs.mkdir(target);
	if (scenario === "entries") {
		await fs.writeFile(join(target, ".hidden"), "");
		await fs.writeFile(join(target, "Alpha"), "");
		await fs.mkdir(join(target, "dir"));
		await fs.symlink("dir", join(target, "link"));
		await fs.symlink("missing", join(target, "broken"));
		await fs.symlink("cycle", join(target, "cycle"));
		await fs.writeFile(Buffer.concat([Buffer.from(`${target}/`), Buffer.from([255])]), "");
	}
	if (scenario === "unreadable") await fs.chmod(target, 0o000);
	return target;
}

async function executeFixture(api: Api, cwd: string, scenario: string): Promise<unknown> {
	const target = await createFixture(cwd, scenario);
	const operations = {
		async exists(path: string) {
			try {
				await fs.access(path);
			} catch {
				return false;
			}
			if (scenario === "remove-after-exists") await fs.rmdir(target);
			return true;
		},
		async stat(path: string) {
			const value = await fs.stat(path);
			if (path === target && scenario === "remove-after-stat") await fs.rmdir(target);
			if (path === target && scenario === "file-after-stat") {
				await fs.rmdir(target);
				await fs.writeFile(target, "replacement");
			}
			return value;
		},
		readdir: fs.readdir,
	};
	try {
		const tool = api.createLsToolDefinition(cwd, { operations });
		return { ok: true, value: await tool.execute("filesystem", { path: "target" }) };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return { ok: false, error: error.message };
	} finally {
		if (scenario === "unreadable") await fs.chmod(target, 0o700);
		await fs.rm(cwd, { recursive: true });
	}
}

void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const scenario = String(value);
		const cwd = join(String(request.cwd), scenario);
		output.push(await executeFixture(module as Api, cwd, scenario));
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`ls filesystem oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
