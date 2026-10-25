import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
const load = (file: string) => import(pathToFileURL(join(root, file)).href);
const { SessionManager } = await load("session-manager.ts");
const control = await load("session-control-db.ts");
const acceptance = await load("runtime-mailbox-acceptance.ts");
const transcript = await load("runtime-mailbox-transcript.ts");
const delivery = await load("runtime-mailbox-delivery.ts");
type Request = {
	id: number;
	op: string;
	dir: string;
	db: string;
	path: string;
	input?: unknown;
	messages?: unknown[];
	text?: string;
	ids?: number[];
	message?: unknown;
	handled?: string[];
	recipientSessionId?: string;
};
async function handle(request: Request): Promise<unknown> {
	if (request.op === "create") {
		const session = SessionManager.create(request.dir, request.dir);
		session.setMetadataControlDbPath(request.db);
		session.persistForRecovery();
		return {
			path: session.getSessionFile(),
			sessionId: session.getSessionId(),
		};
	}
	if (request.op === "deliveryTrace") {
		const session = SessionManager.create(request.dir, request.dir);
		session.setMetadataControlDbPath(request.db);
		session.persistForRecovery();
		const trace: { id: string; acceptedCount: number }[] = [];
		let prompt: string | null = null;
		await delivery.deliverClaimedRuntimeMailboxMessages({
			controlDbPath: request.db,
			messages: request.messages,
			recipientSessionId: request.recipientSessionId,
			sessionManager: session,
			intercept: async (message: { storeRef: { messageId: string } }) => {
				trace.push({
					id: message.storeRef.messageId,
					acceptedCount: transcript.readAcceptedRuntimeMailboxStoreRefs(
						session.getSessionFile(),
					).length,
				});
				return request.handled?.includes(message.storeRef.messageId) ?? false;
			},
			onAccepted: () => {},
			prompt: async (text: string, accept: (message: unknown) => void) => {
				prompt = text;
				accept(delivery.createRuntimeMailboxUserMessage(text, "extension"));
			},
		});
		return {
			trace,
			prompt,
			refs: transcript.readAcceptedRuntimeMailboxStoreRefs(
				session.getSessionFile(),
			),
		};
	}
	if (request.op === "toolDelivery")
		return acceptance.readRuntimeMailboxToolDelivery(request.message) ?? null;
	if (request.op === "enqueue")
		return control.enqueueStoredRuntimeMailboxMessage(
			request.db,
			request.input,
		);
	if (request.op === "accept" || request.op === "acceptRaw") {
		const session = SessionManager.open(request.path);
		const recipient = { sessionId: session.getSessionId(), agentId: null };
		control.registerRuntimeMailboxListener(
			request.db,
			recipient,
			process.pid,
			request.path,
		);
		const messages = control.takeRuntimeMailboxMessagesForDelivery(
			request.db,
			recipient,
			() => true,
			10,
		);
		session.setMetadataControlDbPath(request.db);
		const message =
			request.op === "acceptRaw"
				? request.message
				: {
						role: "user",
						content: [{ type: "text", text: request.text }],
						inputSource: "extension",
						timestamp: 1,
					};
		try {
			acceptance.acceptRuntimeMailboxMessages(
				session,
				request.db,
				messages,
				message,
			);
		} finally {
			if (request.op === "acceptRaw") {
				acceptance.releaseUnacceptedRuntimeMailboxMessages(
					session,
					request.db,
					messages,
				);
			}
		}
		return {
			persisted: acceptance.isRuntimeMailboxMessagePersisted(message),
			refs: transcript.readAcceptedRuntimeMailboxStoreRefs(request.path),
		};
	}
	const session = SessionManager.openReadOnly(request.path);
	return {
		refs: transcript.readAcceptedRuntimeMailboxStoreRefs(request.path),
		messages: session
			.getEntries()
			.filter((entry: { type: string }) => entry.type === "message")
			.map((entry: { message: unknown }) => entry.message),
		remaining: request.ids?.map(
			(id: number) => control.readRuntimeMailboxMessage(request.db, id) ?? null,
		),
	};
}
for await (const line of createInterface({ input: process.stdin })) {
	const request: Request = JSON.parse(line);
	try {
		console.log(
			JSON.stringify({
				id: request.id,
				ok: true,
				output: await handle(request),
			}),
		);
	} catch (error) {
		console.log(
			JSON.stringify({ id: request.id, ok: false, error: String(error) }),
		);
	}
}
