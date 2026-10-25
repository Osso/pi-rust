import { dirname, join } from "node:path";
import { setImmediate } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { format } from "node:util";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Pending = { resolve(value: unknown): void; reject(value: unknown): void };
type Mailbox = {
	id: number;
	recipient: unknown;
	sender: unknown;
	kind: string;
	body: string;
	storeRef: unknown;
	status: string;
};
async function waitUntil(check: () => boolean) {
	const deadline = Date.now() + 5000;
	while (!check()) {
		if (Date.now() > deadline)
			throw new Error("notify throw fixture timed out");
		await setImmediate();
	}
}

void runOracle(async (module, request) => {
	const source = process.argv[2];
	if (
		!source ||
		typeof request.db !== "string" ||
		typeof request.sessionFile !== "string" ||
		typeof request.nextSessionFile !== "string"
	)
		throw new Error("private queue paths required");
	const sessions = (await import(
		pathToFileURL(join(dirname(source), "../session-manager.ts")).href
	)) as {
		SessionManager: {
			open(
				path: string,
				directory: string,
				cwd: string,
			): { setSessionFile(path: string): void };
		};
	};
	const db = (await import(
		pathToFileURL(join(dirname(source), "../session-control-db.ts")).href
	)) as { listRuntimeMailboxMessages(path: string): Mailbox[] };
	const path = request.db;
	const manager = sessions.SessionManager.open(
		request.sessionFile,
		dirname(request.sessionFile),
		"/repo",
	);
	db.listRuntimeMailboxMessages(path);
	const pending: Pending[] = [];
	const notifications: unknown[] = [];
	const consoleLines: string[] = [];
	const previous = console.error;
	console.error = (...values: unknown[]) => {
		consoleLines.push(Buffer.from(format(...values), "utf8").toString("utf8"));
	};
	try {
		const context = {
			cwd: "/repo",
			mode: "tui",
			hasUI: true,
			controlDbPath: path,
			sessionManager: manager,
			ui: {
				custom: () =>
					new Promise((resolve, reject) => {
						pending.push({ resolve, reject });
					}),
				notify: (body: string, kind: string) => {
					notifications.push({ body, kind });
					throw request.notifyError;
				},
			},
		};
		const definition = (
			module as {
				createAskQuestionsToolDefinition(): {
					execute(
						id: string,
						params: unknown,
						signal: undefined,
						update: undefined,
						context: unknown,
					): Promise<unknown>;
				};
			}
		).createAskQuestionsToolDefinition();
		const params = {
			blocking: false,
			questions: [{ question: request.question }],
			metadata: request.metadata,
		};
		const queued = [];
		for (const id of ["one", "two"])
			queued.push(
				await definition.execute(id, params, undefined, undefined, context),
			);
		await waitUntil(() => pending.length === 1);
		manager.setSessionFile(request.nextSessionFile);
		pending[0].reject(request.reason);
		await waitUntil(() => pending.length === 2 && consoleLines.length === 1);
		const beforeSecondCount = db.listRuntimeMailboxMessages(path).length;
		pending[1].resolve(request.answer);
		await waitUntil(() => db.listRuntimeMailboxMessages(path).length === 1);
		return {
			queued,
			notifications,
			entered: pending.length,
			beforeSecondCount,
			consoleLines,
			messages: db
				.listRuntimeMailboxMessages(path)
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
	} finally {
		console.error = previous;
	}
}).catch((error: unknown) => {
	process.stderr.write(
		`question notify throw oracle failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
