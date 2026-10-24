import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";
type Api = { createBashToolDefinition(cwd: string, options: unknown): { execute(id: string, args: unknown, signal: AbortSignal): Promise<unknown> } };
void runOracle(async (module, request) => {
	const cwd = String(request.cwd);
	const ready = join(cwd, "source-ready");
	const survived = join(cwd, "source-survived");
	const child = spawn("/bin/sh", ["-c", `trap '' TERM; printf ready > '${ready}'; sleep .25; printf survived > '${survived}'`], { detached: true, stdio: "ignore" });
	const exit = new Promise(resolve => child.once("exit", () => resolve(undefined)));
	const deadline = Date.now() + 3000;
	while (!existsSync(ready)) {
		if (Date.now() >= deadline) throw new Error("TERM-resistant fixture did not start");
		await new Promise(resolve => setTimeout(resolve, 5));
	}
	const processModule = await import(pathToFileURL(join(dirname(process.argv[2]), "../runtime-process.ts")).href);
	const runner = { runnerPid: child.pid, processIdentity: processModule.readProcessIdentity(child.pid), artifacts: { directory: cwd, outputPath: join(cwd, "missing.log") }, foregroundCompletionPath: join(cwd, "missing-completion"), manifestPath: join(cwd, "missing-manifest"), jobId: "probe", activationPath: join(cwd, "activation.json") };
	const lifecycle = { findBashJobByToolCallId: () => undefined, launchBash: () => runner };
	const tool = (module as Api).createBashToolDefinition(cwd, { backgroundJobs: { lifecycle, store: {} }, detach: { signal: new AbortController().signal } });
	const controller = new AbortController(); controller.abort();
	try { await tool.execute("terminate", { command: "unused" }, controller.signal); throw new Error("aborted runner unexpectedly completed"); }
	catch (error) { if (!(error instanceof Error) || error.message !== "Command aborted") throw error; }
	await exit;
	return { survived: existsSync(survived), exitCode: child.exitCode, signal: child.signalCode };
}).catch((error: unknown) => { process.stderr.write(`bash termination oracle failed: ${String(error)}\n`); process.exitCode = 1; });
