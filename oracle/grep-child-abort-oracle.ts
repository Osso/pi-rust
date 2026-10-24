import * as fs from "node:fs/promises";
import { join } from "node:path";
import { setTimeout } from "node:timers/promises";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = { createGrepToolDefinition(cwd: string): { execute(id: string, args: unknown, signal: AbortSignal): Promise<unknown> } };
type Fixture = { directory: string; closed?: boolean; afterExit?: boolean; limit?: boolean };
async function waitForFile(path: string, milliseconds: number): Promise<boolean> {
	const deadline = Date.now() + milliseconds;
	do {
		try { await fs.access(path); return true; }
		catch (error) { if ((error as { code?: string }).code !== "ENOENT") throw error; }
		await setTimeout(5);
	} while (Date.now() < deadline);
	return false;
}
async function executeAbort(api: Api, cwd: string, fixture: Fixture): Promise<unknown> {
	process.env.PI_FAKE_RG_CHILD = JSON.stringify(fixture);
	const controller = new AbortController();
	let settled = false;
	const result = api.createGrepToolDefinition(cwd).execute("grep-child", { pattern: "x", ...(fixture.limit ? { limit: 1 } : {}) }, controller.signal)
		.then(value => ({ ok: true, value }), error => ({ ok: false, error: error.message }))
		.then(value => { settled = true; return value; });
	try {
		if (!(await waitForFile(join(fixture.directory, "ready"), 5000))) throw new Error("rg did not become ready");
		const limitStopped = fixture.limit ? await waitForFile(join(fixture.directory, "sigterm"), 300) : null;
		controller.abort({ ignored: "reason" });
		const sigterm = await waitForFile(join(fixture.directory, "sigterm"), 300);
		const settledBeforeRelease = settled;
		await fs.writeFile(join(fixture.directory, "finish"), "finish");
		const observed = await result;
		const released = await waitForFile(join(fixture.directory, "released"), 300);
		const descendantSignalled = await waitForFile(join(fixture.directory, "descendant-sigterm"), 30);
		return { result: observed, sigterm, settledBeforeRelease, released, limitStopped, descendantSignalled };
	} finally {
		await fs.writeFile(join(fixture.directory, "finish"), "finish");
	}
}
void runOracle(async (module, request) => {
	const output = [];
	for (const input of requireCases(request)) output.push(await executeAbort(module as Api, String(request.cwd), input as Fixture));
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`grep child abort oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
