import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { writeFile, open } from "node:fs/promises";
import { execFileSync } from "node:child_process";
async function main(): Promise<void> {
	const { NodeExecutionEnv } = await import(
		pathToFileURL(process.argv[2]!).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		const env = new NodeExecutionEnv({ cwd: input.root });
		let result: unknown;
		if (input.fifo) {
			const path = `${input.root}/source.fifo`;
			execFileSync("mkfifo", [path]);
			const writer = await open(path, "r+");
			const controller = new AbortController();
			const read = env.readTextLines(path, {
				maxLines: 1,
				abortSignal: controller.signal,
			});
			await writer.write(input.abort ? "partial" : "first\n");
			if (input.abort) controller.abort();
			result = await read;
			await writer.close();
		} else {
			if (!input.missing)
				await writeFile(`${input.root}/source.txt`, input.content);
			result = await env.readTextLines(
				"source.txt",
				input.limit === undefined ? {} : { maxLines: Number(input.limit) },
			);
		}
		if (input.abort && !(result as any).ok) {
			const error = (result as any).error;
			result = { code: error.code, message: error.message };
		}
		console.log(JSON.stringify(result));
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
