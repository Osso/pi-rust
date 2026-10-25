import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type StoreApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/multi-agent-store.ts");
type ManagerApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/session-manager.ts");
type ControlApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/session-control-db.ts");
type Agent =
	import("@pi-oracle-source/coding-agent/src/core/multi-agent-store.ts").AgentSnapshot;
type Operation =
	| "outbound"
	| "contact"
	| "sendMain"
	| "sendChild"
	| "notify"
	| "steer";
type OutboundInput = Parameters<
	StoreApi["MultiAgentStore"]["prototype"]["recordOutboundSessionMessage"]
>[0];
type Request = {
	id: number;
	op: Operation | "setup";
	scenario: Operation;
	dir: string;
	db: string;
	now: string;
	input: OutboundInput;
};
const root = process.argv[2];
const stores: StoreApi = await import(
	pathToFileURL(join(root, "multi-agent-store.ts")).href
);
const sessions: ManagerApi = await import(
	pathToFileURL(join(root, "session-manager.ts")).href
);
const control: ControlApi = await import(
	pathToFileURL(join(root, "session-control-db.ts")).href
);

function createAgent(id: string, parentId: string, now: string): Agent {
	return {
		id,
		parentId,
		displayName: id,
		agentType: "worker",
		lifecycle: "running",
		revision: 1,
		createdAt: now,
		updatedAt: now,
		cwd: "/fixture",
		permission: { policy: "on-request", narrowed: true },
	};
}

function execute(
	store: InstanceType<StoreApi["MultiAgentStore"]>,
	request: Request,
) {
	const first = createAgent("agent_1", "main", request.now);
	switch (request.op) {
		case "outbound":
			return store.recordOutboundSessionMessage(request.input);
		case "contact": {
			const result = store.contactParent("agent_2", request.input);
			if (!result.ok) throw new Error(`contact failed: ${result.error}`);
			return result.message;
		}
		case "sendMain": {
			const result = store.sendMainThreadMailboxMessage(request.input);
			if (!result.ok) throw new Error(`sendMain failed: ${result.error}`);
			return result.message;
		}
		case "sendChild": {
			const result = store.sendMailboxMessage("agent_1", 1, request.input);
			if (!result.ok) throw new Error(`sendChild failed: ${result.error}`);
			return result.message;
		}
		case "notify":
			store.publishTerminalOutboxSnapshot({
				...first,
				lifecycle: "completed",
				revision: 2,
				result: { summary: "Done", fileRefs: request.input.fileRefs },
			});
			return store.listMailboxMessages()[0];
		case "steer":
			return store.prepareSteeringMessageForLifecycleCoordinator(
				"agent_1",
				request.input,
			);
		case "setup":
			throw new Error("setup is not a store operation");
	}
}

function handle(request: Request): unknown {
	const manager = sessions.SessionManager.create(request.dir, request.dir);
	manager.setMetadataControlDbPath(request.db);
	manager.persistForRecovery();
	const path = manager.getSessionFile();
	if (!path) throw new Error("fixture session has no transcript");
	if (request.scenario !== "outbound")
		for (const agent of [
			createAgent("agent_1", "main", request.now),
			createAgent("agent_2", "agent_1", request.now),
		]) {
			control.bootstrapMultiAgentAgent(request.db, path, agent.id, agent);
		}
	if (request.op === "setup") return { path };
	const store = stores.MultiAgentStore.fromSessionManager(manager, {
		now: () => request.now,
	});
	const message = execute(store, request);
	if (!message) throw new Error("fixture operation did not return a message");
	const stored =
		control.withControlDb(request.db, (db) =>
			db
				.prepare(
					"SELECT data FROM multi_agent_mailbox_messages WHERE session_path=? AND message_id=?",
				)
				.get(path, message.id),
		) ?? null;
	return { returned: JSON.stringify(message), stored };
}

for await (const line of createInterface({ input: process.stdin })) {
	const request: Request = JSON.parse(line);
	try {
		console.log(
			JSON.stringify({ id: request.id, ok: true, output: handle(request) }),
		);
	} catch (error) {
		console.log(
			JSON.stringify({ id: request.id, ok: false, error: String(error) }),
		);
	}
}
