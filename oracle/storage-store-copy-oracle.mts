import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type StoreApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/multi-agent-store.ts");
const { MultiAgentStore }: StoreApi = await import(
	pathToFileURL(process.argv[2]).href
);

import type {
	AgentMailboxMessage as Message,
	AgentSnapshot as Snapshot,
} from "@pi-oracle-source/coding-agent/src/core/multi-agent-store.ts";

for await (const line of createInterface({ input: process.stdin })) {
	const request: {
		id: number;
		op: "snapshot" | "terminal" | "steering";
		initial: Snapshot;
		agent: Snapshot;
		message: Message;
		now: string;
	} = JSON.parse(line);
	let calls = 0;
	const store = new MultiAgentStore({
		now: () => {
			calls++;
			return request.now;
		},
	});
	store.publishLifecycleCoordinatorSnapshot(request.initial);
	let result: unknown;
	try {
		switch (request.op) {
			case "snapshot":
				store.publishLifecycleCoordinatorSnapshot(request.agent);
				break;
			case "terminal":
				store.publishTerminalOutboxSnapshot(request.agent);
				break;
			case "steering":
				store.publishLifecycleCoordinatorSteering(
					request.agent,
					request.message,
				);
				break;
		}
		result = { ok: true };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		result = { ok: false, error: { name: error.name, message: error.message } };
	}
	const samples = calls;
	console.log(
		JSON.stringify({
			id: request.id,
			result,
			samples,
			agents: store.listAgents(),
			agentsWire: JSON.stringify(store.listAgents()),
			messages: store.listMailboxMessages(),
		}),
	);
}
