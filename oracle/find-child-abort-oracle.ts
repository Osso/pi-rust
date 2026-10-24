import * as fs from "node:fs/promises";
import { join } from "node:path";
import { setTimeout } from "node:timers/promises";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createFindToolDefinition(cwd: string): {
		execute(id: string, args: unknown, signal: AbortSignal): Promise<unknown>;
	};
};

async function waitForFile(path: string, milliseconds: number): Promise<boolean> {
	const deadline = Date.now() + milliseconds;
	do {
		try { await fs.access(path); return true; }
		catch (error) { if ((error as { code?: string }).code !== "ENOENT") throw error; }
		await setTimeout(5);
	} while (Date.now() < deadline);
	return false;
}

async function stopFixtureChild(directory: string): Promise<void> {
	const pid = Number(await fs.readFile(join(directory, "pid"), "utf8"));
	try { process.kill(pid, "SIGKILL"); }
	catch (error) { if ((error as { code?: string }).code !== "ESRCH") throw error; }
}

async function executeAbort(api: Api, cwd: string, input: unknown): Promise<unknown> {
	const fixture = input as { directory: string; closed: boolean };
	process.env.PI_FAKE_FD_ABORT = JSON.stringify(fixture);
	const controller = new AbortController();
	const result = api.createFindToolDefinition(cwd).execute("fd-abort", { pattern: "*" }, controller.signal)
		.then(value => ({ ok: true, value }), error => ({ ok: false, error: error.message }));
	try {
		if (!(await waitForFile(join(fixture.directory, "ready"), 5000))) throw new Error("fd did not become ready");
		controller.abort({ ignored: "reason" });
		const observed = await result;
		const sigterm = await waitForFile(join(fixture.directory, "sigterm"), 300);
		return { result: observed, sigterm };
	} finally {
		await stopFixtureChild(fixture.directory);
	}
}

void runOracle(async (module, request) => {
	const output = [];
	for (const input of requireCases(request)) output.push(await executeAbort(module as Api, String(request.cwd), input));
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`find child abort oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
