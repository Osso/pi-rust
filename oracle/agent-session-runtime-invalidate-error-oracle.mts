import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { AgentSessionRuntime } = await import(pathToFileURL(`${root}/core/agent-session-runtime.ts`).href);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const old = { disposed: false, extensionRunner: { hasHandlers: () => false },
			sessionManager: { getMetadataControlDbPath: () => undefined, getSessionFile: () => undefined },
			dispose() { this.disposed = true; } };
		const receiver = Object.create(AgentSessionRuntime.prototype);
		receiver.lifecycleTransitionTail = Promise.resolve();
		receiver._session = old;
		let fail = true;
		receiver.setBeforeSessionInvalidate(() => {
			if (fail) { fail = false; throw new Error(String.fromCharCode(...request.error)); }
		});
		receiver.newSessionUnlocked = async () => {
			await receiver.teardownCurrent("new");
			receiver._session = {};
			return { cancelled: false };
		};
		let error: number[] = [];
		try { await receiver.newSession(); } catch (failure) {
			if (!(failure instanceof Error)) throw failure;
			error = Array.from({ length: failure.message.length }, (_, i) => failure.message.charCodeAt(i));
		}
		const afterError = { sameSession: receiver.session === old, disposed: old.disposed };
		await receiver.newSession();
		console.log(JSON.stringify({ ok: true, output: { error, afterError, afterNext: { changed: receiver.session !== old, disposed: old.disposed } } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
