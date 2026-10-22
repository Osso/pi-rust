import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const sourceUrl = pathToFileURL(process.argv[2]!);
	const capture = await import(sourceUrl.href);
	const types = await import(new URL("../types.ts", sourceUrl).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line) as { chunks: string[]; failCreate?: boolean; failAppend?: boolean; failCallback?: boolean; execError?: string; abort?: boolean };
		let file = "";
		const observed: string[] = [];
		const env = {
			async createTempFile() {
				return request.failCreate ? types.err(new types.FileError("permission_denied", "create denied")) : types.ok("/capture.log");
			},
			async appendFile(_path: string, text: string) {
				if (request.failAppend) return types.err(new types.FileError("permission_denied", "append denied"));
				file += text;
				return types.ok(undefined);
			},
			async exec(_command: string, options: { onStdout: (text: string) => void }) {
				for (const chunk of request.chunks) options.onStdout(chunk);
				return request.execError ? types.err(new types.ExecutionError(request.execError, "execution failed")) : types.ok({ stdout: "", stderr: "", exitCode: 7 });
			},
		};
		const controller = new AbortController();
		if (request.abort) controller.abort();
		const result = await capture.executeShellWithCapture(env, "fixture", {
			abortSignal: controller.signal,
			onChunk(text: string) {
				observed.push(text);
				if (request.failCallback) throw new Error("callback failed");
			},
		});
		const output = result.ok ? { ok: true, value: result.value } : { ok: false, error: { name: result.error.name, code: result.error.code, message: result.error.message } };
		process.stdout.write(`${JSON.stringify({ wire: JSON.stringify({ result: output, file, observed }) })}\n`);
	}
}
void main().catch((error: unknown) => { console.error(error); process.exitCode = 1; });
