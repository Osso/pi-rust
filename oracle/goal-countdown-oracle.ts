import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Refresher = {
	bind(session: string, at: string, callback: () => boolean): () => void;
	start(ctx: unknown, at: string): void;
	clearSession(session: string): void;
	clearAll(): void;
};
runOracle((api) => {
	let now = 1700000000000;
	let next = 0;
	const timers = new Map<number, { at: number; callback: () => void }>();
	const oldNow = Date.now;
	const oldSet = globalThis.setTimeout;
	const oldClear = globalThis.clearTimeout;
	Date.now = () => now;
	globalThis.setTimeout = ((callback: () => void, delay: number) => {
		timers.set(++next, { at: now + delay, callback });
		return next;
	}) as unknown as typeof setTimeout;
	globalThis.clearTimeout = ((id: number) => {
		timers.delete(id);
	}) as unknown as typeof clearTimeout;
	const refresher = (api.createWaitCountdownRefresher as () => Refresher)();
	const events: string[] = [];
	const ctx = { sessionManager: { getSessionId: () => "session" } };
	try {
		const at = new Date(now + 100).toISOString();
		const oldCleanup = refresher.bind("session", at, () => {
			events.push("old");
			return true;
		});
		let calls = 0;
		refresher.bind("session", at, () => {
			events.push("new");
			return ++calls > 1;
		});
		oldCleanup();
		refresher.start(ctx, at);
		for (let index = 0; index < 2; index++) {
			const [id, timer] = [...timers.entries()][0];
			timers.delete(id);
			now = timer.at;
			timer.callback();
		}
		refresher.start(ctx, "invalid");
		refresher.start(ctx, at);
		const later = new Date(now + 100).toISOString();
		refresher.bind("session", later, () => {
			events.push("cleared");
			return true;
		});
		refresher.start(ctx, later);
		refresher.clearSession("session");
		for (const timer of timers.values()) timer.callback();
		return events;
	} finally {
		refresher.clearAll();
		Date.now = oldNow;
		globalThis.setTimeout = oldSet;
		globalThis.clearTimeout = oldClear;
	}
}).catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
