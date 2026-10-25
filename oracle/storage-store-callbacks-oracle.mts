import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type StoreApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/multi-agent-store.ts");
type ManagerApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/session-manager.ts");
type ControlApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/session-control-db.ts");
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
type Request = {
	id: number;
	op:
		| "restoreSetup"
		| "restore"
		| "outbound"
		| "abort"
		| "slots"
		| "binding"
		| "steering";
	dir: string;
	db: string;
	now: string;
	count: number | null;
	throwAt?: number;
	failure?: unknown;
	typed?: boolean;
	ids: string[];
	memory?: boolean;
	noDb?: boolean;
	expired?: boolean;
};

function rejection(request: Request): unknown {
	return request.typed
		? Object.assign(new TypeError("callback failed"), { code: "FIXTURE" })
		: request.failure;
}
function project(error: unknown): unknown {
	if (!(error instanceof Error)) return error;
	return {
		name: error.name,
		message: error.message,
		...Object.fromEntries(
			Object.entries(error).filter(([key]) => key !== "stack"),
		),
	};
}
function capture<T>(call: () => T): unknown {
	try {
		return { ok: true, value: call() };
	} catch (error) {
		return { ok: false, error: project(error) };
	}
}
function createSession(request: Request) {
	const manager = sessions.SessionManager.create(request.dir, request.dir);
	manager.setMetadataControlDbPath(request.db);
	manager.persistForRecovery();
	return manager;
}
function prepareRestore(request: Request) {
	const manager = createSession(request);
	const path = manager.getSessionFile();
	if (!path) throw new Error("fixture session has no transcript");
	if (request.count !== null) {
		control.allocateMultiAgentCounter(request.db, path, "agent");
		const seed = new stores.MultiAgentStore({ now: () => request.now });
		seed.setPersistenceSessionManager(manager);
		for (let index = 0; index < request.count; index++)
			seed.recordOutboundSessionMessage({
				fromAgentId: "main",
				toAgentId: "remote",
				body: `message ${index}`,
			});
	}
	return manager;
}
function clockCase(request: Request): unknown {
	const manager =
		request.op === "restore" ? prepareRestore(request) : undefined;
	let calls = 0;
	let enabled = true;
	const store = new stores.MultiAgentStore({
		now: () => {
			calls++;
			if (enabled && calls === request.throwAt) throw rejection(request);
			return request.now;
		},
	});
	const record = () =>
		store.recordOutboundSessionMessage({
			fromAgentId: "main",
			toAgentId: "remote",
			body: "probe",
		});
	const first = capture(() => {
		if (manager) {
			store.restoreFromSessionManager(manager);
			return null;
		}
		return record();
	});
	const samples = calls;
	enabled = false;
	const probe = manager ? null : record();
	return { first, samples, probe, messages: store.listMailboxMessages() };
}
function steeringCase(request: Request): unknown {
	let calls = 0;
	let enabled = true;
	const store = new stores.MultiAgentStore({
		now: () => {
			calls++;
			if (enabled && calls === request.throwAt) throw rejection(request);
			return request.now;
		},
	});
	const running = {
		id: "child",
		parentId: "main",
		displayName: "child",
		agentType: "worker",
		lifecycle: "running" as const,
		revision: 1,
		createdAt: request.now,
		updatedAt: request.now,
		cwd: "/fixture",
		permission: { policy: "on-request", narrowed: true },
	};
	store.publishLifecycleCoordinatorSnapshot(running);
	const message = {
		id: "steering",
		fromAgentId: "main",
		toAgentId: "child",
		kind: "steer" as const,
		status: "pending" as const,
		createdAt: request.expired ? "2020-01-01T00:00:00.000Z" : request.now,
		updatedAt: request.now,
		body: "Steer 漢😀",
	};
	const first = capture(() => {
		store.publishLifecycleCoordinatorSteering(
			{ ...running, lifecycle: "steering_pending", revision: 2 },
			message,
		);
		return null;
	});
	const samples = calls;
	enabled = false;
	return {
		first,
		samples,
		agent: store.getAgent("child"),
		messages: store.listMailboxMessages(),
	};
}
function abortCase(request: Request): unknown {
	const store = new stores.MultiAgentStore();
	let calls = 0;
	store.registerAgentAbortHandler("child", () => {
		calls++;
		throw rejection(request);
	});
	const first = capture(() => store.abortAgentHandle("child"));
	return { first, calls, second: store.abortAgentHandle("child") };
}
function slotCase(request: Request): unknown {
	const store = new stores.MultiAgentStore({ now: () => request.now });
	for (const id of [...request.ids, "target"])
		store.publishLifecycleCoordinatorSnapshot({
			id,
			parentId: "main",
			displayName: id,
			agentType: "worker",
			lifecycle: "running",
			revision: 1,
			createdAt: request.now,
			updatedAt: request.now,
			cwd: "/fixture",
			permission: { policy: "on-request", narrowed: true },
			...(id === "target" ? {} : { slot: { index: 1, pinned: true } }),
		});
	const order = store.getProjectionSnapshot().slots.map((slot) => slot.agentId);
	const selection = store.selectAgentSlot(1)?.id;
	const conflict = store.pinAgentSlot("target", 1, 1);
	if (conflict.ok || conflict.error !== "slot_conflict")
		throw new Error("fixture expected slot conflict");
	return { order, selection, occupant: conflict.occupant.id };
}
function bindingCase(request: Request): unknown {
	const manager = request.memory
		? sessions.SessionManager.inMemory(request.dir)
		: sessions.SessionManager.create(request.dir, request.dir);
	manager.setMetadataControlDbPath(request.noDb ? undefined : request.db);
	if (!request.memory) manager.persistForRecovery();
	const store = new stores.MultiAgentStore();
	store.setPersistenceSessionManager(manager);
	return {
		path: manager.getSessionFile() ?? null,
		persistence: store.getPersistenceTarget() ?? null,
	};
}
function handle(request: Request): unknown {
	switch (request.op) {
		case "restoreSetup":
			return { path: prepareRestore(request).getSessionFile() };
		case "restore":
		case "outbound":
			return clockCase(request);
		case "abort":
			return abortCase(request);
		case "steering":
			return steeringCase(request);
		case "slots":
			return slotCase(request);
		case "binding":
			return bindingCase(request);
	}
}
for await (const line of createInterface({ input: process.stdin })) {
	const request: Request = JSON.parse(line);
	console.log(
		JSON.stringify({ id: request.id, ok: true, output: handle(request) }),
	);
}
