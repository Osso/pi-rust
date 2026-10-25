import { DatabaseSync } from "node:sqlite";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";
runOracle((api, request) => {
	if (request.operation === "parse") {
		return (api.parseDetachedPyrunBridgeRequest as (message: unknown) => unknown)({ body: request.body }) ?? null;
	}
	if (request.operation === "response") {
		const input = request.input as { controlDbPath: string; request: { identity: {jobId: string}; requestId: string } };
		(api.enqueueDetachedPyrunBridgeResponse as (input: unknown) => void)(input);
		const db = new DatabaseSync(input.controlDbPath);
		try {
			const row = db.prepare("SELECT data FROM multi_agent_mailbox_messages WHERE message_id = ?").get(`pyrun-response:${input.request.identity.jobId}:${input.request.requestId}`) as { data: string };
			const message = JSON.parse(row.data) as {createdAt: string}; message.createdAt = "<created-at>";
			return JSON.stringify(message);
		} finally { db.close(); }
	}
	const input = request.input as { controlDbPath: string; identity: {jobId: string} };
	const id = (api.enqueueDetachedPyrunBridgeRequest as (input: unknown) => string)(input);
	const db = new DatabaseSync(input.controlDbPath);
	try {
		const row = db.prepare("SELECT data FROM multi_agent_mailbox_messages WHERE message_id = ?").get(`pyrun-request:${input.identity.jobId}:${id}`) as { data: string };
		const message = JSON.parse(row.data) as { body: string; id: string; createdAt: string };
		const body = JSON.parse(message.body) as { requestId: string };
		body.requestId = "<request>";
		message.body = JSON.stringify(body);
		message.id = `pyrun-request:${input.identity.jobId}:<request>`;
		message.createdAt = "<created-at>";
		return JSON.stringify(message);
	} finally { db.close(); }
}).catch(error => { console.error(error); process.exitCode = 1; });
