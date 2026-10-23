import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import * as fs from "node:fs";
import { join } from "node:path";
const pause = () => new Promise((resolve) => setTimeout(resolve, 5));
async function main(): Promise<void> {
	const { CombinedAutocompleteProvider } = await import(
		pathToFileURL(`${process.argv[2]}/autocomplete.ts`).href
	);
	async function run(request: any): Promise<unknown> {
		const controller = new AbortController();
		const provider = new CombinedAutocompleteProvider(
			[],
			request.base,
			request.fd,
		);
		const pending = provider.getSuggestions(["@file"], 0, 5, {
			signal: controller.signal,
		});
		const parentPath = join(request.base, "parent.pid");
		const heldPath = join(request.base, "held.pid");
		const releasePath = join(request.base, "release");
		const deadline = Date.now() + 5000;
		const gone = (pid: number) => {
			try {
				process.kill(pid, 0);
				return false;
			} catch {
				return true;
			}
		};
		try {
			while (
				(!fs.existsSync(parentPath) || !fs.existsSync(heldPath)) &&
				Date.now() < deadline
			)
				await pause();
			if (!fs.existsSync(parentPath) || !fs.existsSync(heldPath))
				throw new Error("fd fixture did not hold its pipe");
			const pid = Number(fs.readFileSync(parentPath, "utf8"));
			while (!gone(pid) && Date.now() < deadline) await pause();
			if (!gone(pid)) throw new Error("fd parent did not exit before abort");
			const pipeHeld = !fs.existsSync(join(request.base, "released"));
			if (request.abort) controller.abort();
			fs.writeFileSync(releasePath, "release");
			const result = await pending;
			return {
				parentGone: true,
				pipeHeld,
				aborted: controller.signal.aborted,
				result: result
					? [
							result.items.map((item: any) => [
								item.value,
								item.label,
								item.description ?? null,
							]),
							result.prefix,
						]
					: null,
			};
		} finally {
			fs.writeFileSync(releasePath, "release");
		}
	}
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			process.stdout.write(
				`${JSON.stringify({ ok: true, output: await run(JSON.parse(line)) })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		}
	}
}
void main();
