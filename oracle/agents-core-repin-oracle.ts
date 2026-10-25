import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

type Request = Record<string, unknown>;
type Listener = () => void;
type SettlementStore = {
	getPersistenceTarget(): undefined;
	getAgent(): { lifecycle: string };
	listDescendants(): Array<{ id: string; parentId: string; lifecycle: string }>;
	subscribeAgentUpdates(listener: Listener): Listener;
};
type SettlementModule = {
	waitForActiveDescendants(store: SettlementStore, id: string, signal: AbortSignal): Promise<void>;
};
type ChildResponseModule = {
	readCurrentChildAssistantText(messages: unknown[], previous: ReadonlySet<unknown>): string | undefined;
};
type DetachedModule = { isDetachedRuntimeAgent(agent: unknown): boolean };

async function load<T>(name: string): Promise<T> {
	const source = process.argv[2];
	if (!source) throw new Error("agents-core source directory is required");
	return import(pathToFileURL(resolve(source, name)).href) as Promise<T>;
}

async function runSettlementAbort(cancelling: boolean): Promise<unknown> {
	const { waitForActiveDescendants } = await load<SettlementModule>("descendant-settlement.ts");
	const controller = new AbortController();
	controller.abort();
	const child = { id: "child", parentId: "parent", lifecycle: "running" };
	const listeners = new Set<Listener>();
	const store: SettlementStore = {
		getPersistenceTarget: () => undefined,
		getAgent: () => ({ lifecycle: cancelling ? "cancelling" : "running" }),
		listDescendants: () => [child],
		subscribeAgentUpdates: (listener) => {
			listeners.add(listener);
			return () => { listeners.delete(listener); };
		},
	};
	let settled = false;
	const waiting = waitForActiveDescendants(store, "parent", controller.signal).then(() => { settled = true; });
	await Promise.resolve();
	const settledBeforeUpdate = settled;
	child.lifecycle = "completed";
	for (const listener of listeners) listener();
	await waiting;
	return { settledBeforeUpdate, listenersAfterFinish: listeners.size };
}

async function execute(request: Request): Promise<unknown> {
	switch (request.op) {
		case "settlementAbort": return runSettlementAbort(request.cancelling === true);
		case "childResponse": {
			const { readCurrentChildAssistantText } = await load<ChildResponseModule>("child-response.ts");
			const messages = request.messages as unknown[];
			const previous = new Set(messages.slice(0, Number(request.previousCount ?? 0)));
			return readCurrentChildAssistantText(messages, previous) ?? null;
		}
		case "isDetached": {
			const { isDetachedRuntimeAgent } = await load<DetachedModule>("detached-runtime-cancellation.ts");
			return isDetachedRuntimeAgent(request.agent);
		}
		default: throw new Error(`Unknown operation ${request.op}`);
	}
}

function errorResult(id: unknown, error: unknown): unknown {
	const name = error instanceof Error ? error.name : "Error";
	const message = error instanceof Error ? error.message : String(error);
	return { id, ok: false, error: { name, message } };
}

async function respond(line: string): Promise<void> {
	let id: unknown = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const output = await execute(request);
		process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify(errorResult(id, error))}\n`);
	}
}

async function main(): Promise<void> {
	const input = createInterface({ input: process.stdin });
	for await (const line of input) await respond(line);
}

void main().catch((error: unknown) => {
	process.stderr.write(`${JSON.stringify(errorResult(null, error))}\n`);
	process.exitCode = 1;
});
