import fs from "node:fs/promises";
import { syncBuiltinESMExports } from "node:module";
import { join } from "node:path";
import { setTimeout } from "node:timers/promises";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Gate = { entered: PromiseWithResolvers<void>; release: PromiseWithResolvers<void>; used: boolean };
let active: Gate | undefined;
const access = fs.access;
fs.access = async (...args: Parameters<typeof access>) => {
	if (active && !active.used && String(args[0]).endsWith("/.git")) {
		active.used = true;
		active.entered.resolve();
		await active.release.promise;
	}
	return access(...args);
};
syncBuiltinESMExports();

type Api = { createFindToolDefinition(cwd: string): { execute(id: string, args: unknown, signal: AbortSignal): Promise<unknown> } };

async function waitForFile(path: string): Promise<boolean> {
	const deadline = Date.now() + 5000;
	do {
		try { await access(path); return true; }
		catch (error) { if ((error as { code?: string }).code !== "ENOENT") throw error; }
		await setTimeout(5);
	} while (Date.now() < deadline);
	return false;
}

void runOracle(async (module, request) => {
	const directory = String(request.directory);
	process.env.PI_FAKE_FD_ABORT = JSON.stringify({ directory, late: true, closed: true });
	const gate = { entered: Promise.withResolvers<void>(), release: Promise.withResolvers<void>(), used: false };
	active = gate;
	const controller = new AbortController();
	const result = (module as Api).createFindToolDefinition(String(request.cwd))
		.execute("late-fd-abort", { pattern: "*" }, controller.signal)
		.then(value => ({ ok: true, value }), error => ({ ok: false, error: error.message }));
	await gate.entered.promise;
	controller.abort({ ignored: "reason" });
	const observed = await result;
	gate.release.resolve();
	const started = await waitForFile(join(directory, "ready"));
	await fs.writeFile(join(directory, "finish"), "finish");
	const naturalExit = await waitForFile(join(directory, "natural-exit"));
	let sigterm = false;
	try { await access(join(directory, "sigterm")); sigterm = true; }
	catch (error) { if ((error as { code?: string }).code !== "ENOENT") throw error; }
	active = undefined;
	return { result: observed, started, naturalExit, sigterm };
}).catch((error: unknown) => {
	process.stderr.write(`find late abort oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
