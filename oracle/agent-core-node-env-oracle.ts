import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { basename, dirname } from "node:path";
import { readFile, rm, open } from "node:fs/promises";
import { execFileSync } from "node:child_process";

function abortReason(
	descriptor:
		| { kind: string; value?: unknown; name?: string; message?: string }
		| undefined,
): unknown {
	if (!descriptor) return undefined;
	if (descriptor.kind === "value") return descriptor.value;
	return Object.assign(new Error(descriptor.message), {
		name: descriptor.name ?? "Error",
	});
}

async function main(): Promise<void> {
	const { NodeExecutionEnv } = await import(
		pathToFileURL(process.argv[2]!).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		const shellEnv = input.shellEnv ?? { CORE_CONFIG: "configured" };
		for (const key of input.undefinedEnv ?? []) shellEnv[key] = undefined;
		const env = new NodeExecutionEnv({
			cwd: input.root,
			shellPath: input.shellPath,
			shellEnv,
		});
		const controller = new AbortController();
		controller.abort();
		let result: unknown;
		if (input.kind === "error-causes") {
			result = [
				(await env.readTextFile("missing\ud800")).error,
				(await env.fileInfo("bad\u0000path")).error,
				(await env.exec("true", { cwd: "missing" })).error,
				(await env.exec("bad\u0000command")).error,
				(
					await env.exec("printf out", {
						onStdout: () => {
							throw new Error("native\ud800cause");
						},
					})
				).error,
			];
		} else if (input.kind === "deleted-cwd-callers") {
			const source = pathToFileURL(process.argv[2]!);
			const [
				{ loadPromptTemplates },
				{ loadSkills },
				{ JsonlSessionRepo },
				{ executeShellWithCapture },
			] = await Promise.all([
				import(new URL("../prompt-templates.ts", source).href),
				import(new URL("../skills.ts", source).href),
				import(new URL("../session/jsonl-repo.ts", source).href),
				import(new URL("../utils/shell-output.ts", source).href),
			]);
			const nativeError = (error: any) => ({
				name: error.name,
				code: error.code,
				errno: error.errno,
				syscall: error.syscall,
				message: error.message,
			});
			const local = new NodeExecutionEnv({ cwd: "base" });
			const operations: Record<string, () => Promise<unknown>> = {
				absolutePath: () => local.absolutePath("child"),
				readTextFile: () => local.readTextFile("child"),
				readTextLines: () => local.readTextLines("child", { maxLines: -1 }),
				readBinaryFile: () => local.readBinaryFile("child"),
				writeFile: () => local.writeFile("child", "a"),
				appendFile: () => local.appendFile("child", "a"),
				fileInfo: () => local.fileInfo("child"),
				listDir: () => local.listDir("child"),
				canonicalPath: () => local.canonicalPath("child"),
				exists: () => local.exists("child"),
				createDir: () => local.createDir("child"),
				remove: () => local.remove("child"),
				exec: () => local.exec("true", { cwd: "child" }),
				joinPath: () => local.joinPath(["a", "b"]),
				prompts: () => loadPromptTemplates(local, ["child"]),
				skills: () => loadSkills(local, ["child"]),
				repo: () =>
					new JsonlSessionRepo({ fs: local, sessionsRoot: "sessions" }).list(),
				capture: async () => {
					const result = await executeShellWithCapture(local, "true", {
						cwd: "child",
					});
					if (result.ok) return result;
					return {
						ok: false,
						error: {
							name: result.error.name,
							code: result.error.code,
							message: result.error.message,
							cause: nativeError(result.error.cause),
						},
					};
				},
			};
			const original = process.cwd();
			process.chdir(input.root);
			await rm(input.root, { recursive: true });
			const outcomes: Record<string, unknown> = {};
			try {
				for (const [name, operation] of Object.entries(operations)) {
					try {
						outcomes[name] = { returned: await operation() };
					} catch (error) {
						outcomes[name] = { rejected: nativeError(error) };
					}
				}
			} finally {
				process.chdir(original);
			}
			result = outcomes;
		} else if (input.kind === "deleted-cwd") {
			const original = process.cwd();
			process.chdir(input.root);
			await rm(input.root, { recursive: true });
			try {
				result = {
					returned: await new NodeExecutionEnv({ cwd: "base" }).absolutePath(
						"child",
					),
				};
			} catch (error: any) {
				result = {
					rejected: {
						name: error.name,
						code: error.code,
						errno: error.errno,
						syscall: error.syscall,
						message: error.message,
					},
				};
			} finally {
				process.chdir(original);
			}
		} else if (input.kind === "temp-errors") {
			const missing = await env.createTempDir("missing-parent/pre");
			const invalid = await env.createTempDir("bad\u0000prefix");
			const file = await env.createTempFile({
				prefix: "missing/",
				suffix: ".end",
			});
			if (file.ok) throw new Error("expected file fixture to fail");
			await rm(dirname(dirname(file.error.path)), { recursive: true });
			result = [missing, invalid, file];
		} else if (input.kind === "write-abort") {
			const path = `${input.root}/write.fifo`;
			execFileSync("mkfifo", [path]);
			const abort = new AbortController();
			const write = env.writeFile("write.fifo", "hello", abort.signal);
			const bytes = new Promise<string>((resolve, reject) =>
				setTimeout(() => {
					abort.abort();
					readFile(path, "utf8").then(resolve, reject);
				}, 30),
			);
			result = { write: await write, text: await bytes };
		} else if (input.kind === "read-lines-abort") {
			const path = `${input.root}/read.fifo`;
			execFileSync("mkfifo", [path]);
			const writer = await open(path, "r+");
			await writer.write("prefix");
			const abort = new AbortController();
			const read = env.readTextLines("read.fifo", {
				abortSignal: abort.signal,
			});
			const release = new Promise<void>((resolve, reject) =>
				setTimeout(() => {
					abort.abort();
					writer.close().then(resolve, reject);
				}, 30),
			);
			result = await read;
			await release;
		} else if (input.kind === "read-abort") {
			const path = `${input.root}/read.fifo`;
			execFileSync("mkfifo", [path]);
			const writer = await open(path, "r+");
			await writer.write("prefix");
			const abort = new AbortController();
			const read = env[input.method]("read.fifo", abort.signal);
			setTimeout(() => {
				abort.abort(abortReason(input.abortReason));
				void writer.close();
			}, 30);
			result = await read;
		} else if (input.kind === "shell-boundary") {
			result = await env.exec(input.command, {
				env: input.extraEnv,
				cwd: input.cwd,
			});
		} else if (input.kind === "temp") {
			const file = await env.createTempFile({ prefix: "pre", suffix: ".end" });
			if (!file.ok) throw file.error;
			const name = basename(file.value);
			const uuid = name.slice(3, -4);
			const text = await readFile(file.value, "utf8");
			const dir = await env.createTempDir("raw\ud800-");
			if (!dir.ok) throw dir.error;
			result = {
				uuid: /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
					uuid,
				),
				prefix: name.startsWith("pre"),
				suffix: name.endsWith(".end"),
				text,
				tempDirectory: /^tmp-[a-zA-Z0-9]{6}$/.test(
					basename(dirname(file.value)),
				),
				rawDirectoryPrefix: basename(dir.value).slice(0, -6),
			};
			await rm(dirname(file.value), { recursive: true });
			await rm(dir.value, { recursive: true });
		} else if (input.kind === "fs") {
			result = [];
			for (const [method, ...args] of input.operations) {
				(result as unknown[]).push(await env[method](...args));
			}
		} else if (input.kind === "paths") {
			result = [];
			for (const path of input.paths)
				(result as unknown[]).push(await env.absolutePath(path));
			for (const parts of input.joins)
				(result as unknown[]).push(await env.joinPath(parts));
		} else if (input.kind === "cleanup") {
			result = await env.exec("printf ready; sleep 0.2; printf done", {
				onStdout: () => {
					void env.cleanup();
				},
			});
		} else if (input.kind === "tail") {
			result = await env.exec("(sleep 0.25; printf late) & printf early");
		} else if (input.kind === "post-exit") {
			const abort = new AbortController();
			let timer: ReturnType<typeof setTimeout> | undefined;
			result = await env.exec("(sleep 0.25; printf late) & printf early", {
				timeout: input.mode === "timeout" ? 0.05 : undefined,
				abortSignal: input.mode === "abort" ? abort.signal : undefined,
				onStdout: (chunk: string) => {
					if (input.mode === "abort" && chunk.includes("early"))
						timer = setTimeout(() => abort.abort(), 50);
					if (input.mode === "callback" && chunk.includes("late"))
						throw new Error("late\ud800failure");
				},
			});
			clearTimeout(timer);
		} else if (input.kind === "env") {
			result = await env.exec('printf \'%s:%s\' "$CORE_CONFIG" "$CORE_EXTRA"', {
				env: { CORE_EXTRA: "extra" },
			});
		} else if (input.kind === "missing-cwd") {
			result = await env.exec("printf never", { cwd: "missing" });
		} else if (input.kind === "ignored-abort") {
			const signal = controller.signal;
			const results = [];
			results.push(await env.appendFile("nested/file", "a", signal));
			results.push(await env.exists("nested/file", signal));
			results.push(await env.createDir("extra", { abortSignal: signal }));
			results.push(
				await env.remove("extra", { recursive: true, abortSignal: signal }),
			);
			results.push(await env.readTextFile("nested/file"));
			result = results;
		} else throw new Error(`Unknown kind: ${input.kind}`);
		console.log(
			JSON.stringify(result, (_key, value) => {
				if (!(value instanceof Error)) return value;
				if (!input.withCauses)
					return {
						code: value.code,
						message: value.message,
						...(value.path === undefined ? {} : { path: value.path }),
					};
				const error: Record<string, unknown> = {
					name: value.name,
					message: value.message,
				};
				for (const key of ["code", "errno", "syscall", "path", "cause"])
					if ((value as any)[key] !== undefined)
						error[key] = (value as any)[key];
				return error;
			}),
		);
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
