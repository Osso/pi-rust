import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { format } from "node:util";

const { MultiAgentStore } = await import(pathToFileURL(process.argv[2]).href);
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const store = new MultiAgentStore();
	const logs: string[] = [];
	const revisions: number[] = [];
	const original = console.error;
	console.error = (...args: unknown[]) => logs.push(format(...args));
	try {
		store.publishLifecycleCoordinatorSnapshot(request.running);
		store.subscribeAgentUpdates(() => {
			throw request.failure;
		});
		store.subscribeAgentUpdates(
			(_previous: unknown, current: { revision: number }) =>
				revisions.push(current.revision),
		);
		store.publishLifecycleCoordinatorSnapshot(request.updated);
		console.log(
			JSON.stringify({
				id: request.id,
				logs,
				revisions,
				agent: store.getAgent(request.running.id),
			}),
		);
	} finally {
		console.error = original;
	}
}
