import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { format } from "node:util";

const { MultiAgentStore } = await import(pathToFileURL(process.argv[2]).href);

function observeLiveMembership(request: {
	kind: string;
	mutation: string;
	running: unknown;
	updated: { updatedAt: string };
}): string[] {
	const store = new MultiAgentStore({ now: () => request.updated.updatedAt });
	store.publishLifecycleCoordinatorSnapshot(request.running);
	const observed: string[] = [];
	const subscribe = (listener: () => void): (() => void) => {
		switch (request.kind) {
			case "update":
				return store.subscribeAgentUpdates(listener);
			case "transition":
				return store.subscribeAgentTransitions(listener);
			case "notification":
				return store.subscribeLifecycleNotifications(listener);
			default:
				throw new Error(`unknown listener kind ${request.kind}`);
		}
	};
	let unsubscribeSecond = () => {};
	const unsubscribeFirst = subscribe(() => {
		observed.push("first");
		if (request.mutation === "remove") unsubscribeSecond();
		else unsubscribeSecond = subscribe(() => observed.push("late"));
	});
	if (request.mutation === "remove")
		unsubscribeSecond = subscribe(() => observed.push("second"));
	if (request.kind === "notification")
		store.publishTerminalOutboxSnapshot(request.updated);
	else store.publishLifecycleCoordinatorSnapshot(request.updated);
	unsubscribeFirst();
	unsubscribeSecond();
	return observed;
}

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	if (request.mutation) {
		console.log(
			JSON.stringify({
				id: request.id,
				observed: observeLiveMembership(request),
			}),
		);
		continue;
	}
	const store = new MultiAgentStore({ now: () => request.updated.updatedAt });
	const logs: string[] = [];
	const observed: unknown[] = [];
	const original = console.error;
	console.error = (...args: unknown[]) => logs.push(format(...args));
	try {
		store.publishLifecycleCoordinatorSnapshot(request.running);
		const failing = () => {
			throw request.failure;
		};
		const observeSnapshot = (
			_previous: unknown,
			current: { revision: number },
		) => observed.push(current.revision);
		switch (request.kind) {
			case "update":
				store.subscribeAgentUpdates(failing);
				store.subscribeAgentUpdates(observeSnapshot);
				store.publishLifecycleCoordinatorSnapshot(request.updated);
				break;
			case "transition":
				store.subscribeAgentTransitions(failing);
				store.subscribeAgentTransitions(observeSnapshot);
				store.publishLifecycleCoordinatorSnapshot(request.updated);
				break;
			case "notification":
				store.subscribeLifecycleNotifications(failing);
				store.subscribeLifecycleNotifications((message: unknown) =>
					observed.push(message),
				);
				store.publishTerminalOutboxSnapshot(request.updated);
				break;
			default:
				throw new Error(`unknown listener kind ${request.kind}`);
		}
		console.log(
			JSON.stringify({
				id: request.id,
				logs,
				observed,
				agent: store.getAgent(request.running.id),
			}),
		);
	} finally {
		console.error = original;
	}
}
