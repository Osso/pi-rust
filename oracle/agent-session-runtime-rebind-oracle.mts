import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { AgentSessionRuntime } = await import(pathToFileURL(`${root}/core/agent-session-runtime.ts`).href);
for await (const _line of createInterface({ input: process.stdin })) {
	try {
		const log: string[] = [];
		let release!: () => void;
		let entered!: () => void;
		const held = new Promise<void>((resolve) => { release = resolve; });
		const started = new Promise<void>((resolve) => { entered = resolve; });
		const receiver = Object.create(AgentSessionRuntime.prototype);
		receiver.lifecycleTransitionTail = Promise.resolve();
		receiver.prepareRelocation = (cwd: string) => cwd;
		receiver.activateRelocation = async (cwd: string) => {
			receiver._session = { cwd };
			await receiver.finishSessionReplacement();
		};
		receiver.setRebindSession(async ({ cwd }: { cwd: string }) => {
			log.push(`start:${cwd}`);
			if (cwd === "first") { entered(); await held; }
			log.push(`end:${cwd}`);
		});
		const first = receiver.relocate("first");
		await started;
		const worker = receiver.relocate("worker");
		const owner = receiver.relocate("owner");
		await new Promise((resolve) => setImmediate(resolve));
		const beforeRelease = [...log];
		release();
		await Promise.all([first, worker, owner]);
		console.log(JSON.stringify({ ok: true, output: { beforeRelease, afterRelease: log } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
