import { dirname, join } from "node:path";
import { setImmediate } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Session = { setSessionFile(path: string): void };
type Mailbox = {
	id: number;
	recipient: unknown;
	sender: unknown;
	kind: string;
	body: string;
	storeRef: unknown;
	status: string;
};
type Definition = {
	execute(
		id: string,
		params: unknown,
		signal: undefined,
		update: undefined,
		context: unknown,
	): Promise<unknown>;
};
type Pending = { resolve(value: unknown): void; reject(reason: unknown): void };

async function waitUntil(predicate: () => boolean): Promise<void> {
	const deadline = Date.now() + 5000;
	while (!predicate()) {
		if (Date.now() > deadline)
			throw new Error("private question queue fixture timed out");
		await setImmediate();
	}
}

async function loadServices() {
	const source = process.argv[2];
	if (!source)
		throw new Error("question queue oracle requires a source module");
	const session = (await import(
		pathToFileURL(join(dirname(source), "../session-manager.ts")).href
	)) as {
		SessionManager: {
			open(path: string, directory: string, cwd: string): Session;
		};
	};
	const db = (await import(
		pathToFileURL(join(dirname(source), "../session-control-db.ts")).href
	)) as {
		listRuntimeMailboxMessages(path: string): Mailbox[];
	};
	return { session: session.SessionManager, db };
}

const services = loadServices();
void runOracle(async (module, request) => {
	if (
		typeof request.db !== "string" ||
		typeof request.sessionFile !== "string" ||
		typeof request.nextSessionFile !== "string"
	)
		throw new Error("queue fixture requires private DB/session paths");
	const { session, db } = await services;
	const manager = session.open(
		request.sessionFile,
		dirname(request.sessionFile),
		"/repo",
	);
	const deliveryError = request.operation === "delivery-error";
	if (!deliveryError) db.listRuntimeMailboxMessages(request.db);
	const pending: Pending[] = [];
	const notifications: unknown[] = [];
	const context = {
		cwd: "/repo",
		mode: "tui",
		hasUI: true,
		controlDbPath: request.db,
		multiAgentAgentId: request.agentId,
		sessionManager: manager,
		ui: {
			custom: () =>
				new Promise((resolve, reject) => {
					pending.push({ resolve, reject });
				}),
			notify: (body: string, kind: string) => {
				notifications.push({ body, kind });
			},
		},
	};
	const definition = (
		module as { createAskQuestionsToolDefinition(): Definition }
	).createAskQuestionsToolDefinition();
	const params = {
		blocking: false,
		questions: [{ question: request.question }],
		metadata: request.metadata,
	};
	const queued = [];
	for (const id of deliveryError ? ["failure"] : ["one", "two"])
		queued.push(
			await definition.execute(id, params, undefined, undefined, context),
		);
	await waitUntil(() => pending.length === 1);
	if (deliveryError) {
		pending[0].resolve(request.answer);
		await waitUntil(() => notifications.length === 1);
		return { queued, notifications, entered: pending.length };
	}
	manager.setSessionFile(request.nextSessionFile);
	pending[0].reject(request.reason);
	await waitUntil(
		() =>
			pending.length === 2 &&
			db.listRuntimeMailboxMessages(request.db as string).length === 1,
	);
	pending[1].resolve(request.answer);
	await waitUntil(
		() => db.listRuntimeMailboxMessages(request.db as string).length === 2,
	);
	return {
		queued,
		notifications,
		entered: pending.length,
		messages: db
			.listRuntimeMailboxMessages(request.db)
			.map(({ id, recipient, sender, kind, body, storeRef, status }) => ({
				id,
				recipient,
				sender,
				kind,
				body,
				storeRef,
				status,
			})),
	};
}).catch((error: unknown) => {
	process.stderr.write(`question queue oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
