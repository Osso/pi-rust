import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { AgentSessionRuntime } = await import(pathToFileURL(`${root}/core/agent-session-runtime.ts`).href);
for await (const _line of createInterface({ input: process.stdin })) {
	try {
		let completed = false;
		const receiver = Object.create(AgentSessionRuntime.prototype);
		receiver.lifecycleTransitionTail = Promise.resolve();
		receiver.prepareRelocation = () => ({});
		receiver.activateRelocation = async () => {};
		receiver._session = { continue: async () => {} };
		receiver.newSessionUnlocked = async () => { completed = true; return { cancelled: false }; };
		await receiver.prepareToolResultRelocation("fixture");
		const queued = receiver.newSession();
		await new Promise((resolve) => setImmediate(resolve));
		const beforeActivation = completed;
		await receiver.activateToolResultRelocation();
		await queued;
		console.log(JSON.stringify({ ok: true, output: { beforeActivation, afterActivation: completed } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
