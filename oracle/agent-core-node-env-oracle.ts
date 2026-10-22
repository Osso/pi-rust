import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const { NodeExecutionEnv } = await import(pathToFileURL(process.argv[2]!).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		const env = new NodeExecutionEnv({ cwd: input.root, shellEnv: { CORE_CONFIG: "configured" } });
		const controller = new AbortController();
		controller.abort();
		let result: unknown;
		if (input.kind === "paths") {
			result = [];
			for (const path of input.paths) (result as unknown[]).push(await env.absolutePath(path));
			for (const parts of input.joins) (result as unknown[]).push(await env.joinPath(parts));
		} else if (input.kind === "cleanup") {
			result = await env.exec("printf ready; sleep 0.2; printf done", { onStdout: () => { void env.cleanup(); } });
		} else if (input.kind === "tail") {
			result = await env.exec("(sleep 0.25; printf late) & printf early");
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
