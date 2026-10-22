import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { basename, dirname } from "node:path";
import { readFile, rm } from "node:fs/promises";

async function main(): Promise<void> {
	const { NodeExecutionEnv } = await import(pathToFileURL(process.argv[2]!).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		const shellEnv = input.shellEnv ?? { CORE_CONFIG: "configured" };
		for (const key of input.undefinedEnv ?? []) shellEnv[key] = undefined;
		const env = new NodeExecutionEnv({ cwd: input.root, shellPath: input.shellPath, shellEnv });
		const controller = new AbortController();
		controller.abort();
		let result: unknown;
		if (input.kind === "shell-boundary") {
			result = await env.exec(input.command, { env: input.extraEnv });
		} else if (input.kind === "temp") {
			const file = await env.createTempFile({ prefix: "pre", suffix: ".end" });
			if (!file.ok) throw file.error;
			const name = basename(file.value);
			const uuid = name.slice(3, -4);
			const text = await readFile(file.value, "utf8");
			const dir = await env.createTempDir("raw\ud800-");
			if (!dir.ok) throw dir.error;
			result = { uuid: /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(uuid), prefix: name.startsWith("pre"), suffix: name.endsWith(".end"), text, tempDirectory: /^tmp-[a-zA-Z0-9]{6}$/.test(basename(dirname(file.value))), rawDirectoryPrefix: basename(dir.value).slice(0, -6) };
			await rm(dirname(file.value), { recursive: true });
			await rm(dir.value, { recursive: true });
		} else if (input.kind === "fs") {
			result = [];
			for (const [method, ...args] of input.operations) {
				(result as unknown[]).push(await env[method](...args));
			}
		} else if (input.kind === "paths") {
			result = [];
			for (const path of input.paths) (result as unknown[]).push(await env.absolutePath(path));
			for (const parts of input.joins) (result as unknown[]).push(await env.joinPath(parts));
		} else if (input.kind === "cleanup") {
			result = await env.exec("printf ready; sleep 0.2; printf done", { onStdout: () => { void env.cleanup(); } });
		} else if (input.kind === "tail") {
			result = await env.exec("(sleep 0.25; printf late) & printf early");
		} else if (input.kind === "post-exit") {
			const abort = new AbortController();
			let timer: ReturnType<typeof setTimeout> | undefined;
			result = await env.exec("(sleep 0.25; printf late) & printf early", {
				timeout: input.mode === "timeout" ? 0.05 : undefined,
				abortSignal: input.mode === "abort" ? abort.signal : undefined,
				onStdout: (chunk: string) => {
					if (input.mode === "abort" && chunk.includes("early")) timer = setTimeout(() => abort.abort(), 50);
					if (input.mode === "callback" && chunk.includes("late")) throw new Error("late\ud800failure");
				},
			});
			clearTimeout(timer);
		} else if (input.kind === "env") {
			result = await env.exec("printf '%s:%s' \"$CORE_CONFIG\" \"$CORE_EXTRA\"", { env: { CORE_EXTRA: "extra" } });
		} else if (input.kind === "missing-cwd") {
			result = await env.exec("printf never", { cwd: "missing" });
		} else if (input.kind === "ignored-abort") {
			const signal = controller.signal;
			const results = [];
			results.push(await env.appendFile("nested/file", "a", signal));
			results.push(await env.exists("nested/file", signal));
			results.push(await env.createDir("extra", { abortSignal: signal }));
			results.push(await env.remove("extra", { recursive: true, abortSignal: signal }));
			results.push(await env.readTextFile("nested/file"));
			result = results;
		} else throw new Error(`Unknown kind: ${input.kind}`);
		console.log(JSON.stringify(result, (_key, value) => value instanceof Error ? { code: value.code, message: value.message, ...(value.path === undefined ? {} : { path: value.path }) } : value));
	}
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
