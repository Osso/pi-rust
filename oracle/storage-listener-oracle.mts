import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { format } from "node:util";

const { MultiAgentStore } = await import(pathToFileURL(process.argv[2]).href);
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
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
