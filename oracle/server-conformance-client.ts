import assert from "node:assert/strict";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import type { ServerMessage } from "@earendil-works/pi-protocol";

type Client = {
	hello(): Promise<ServerMessage>;
	request(command: unknown, id?: string): Promise<{
		ok: boolean;
		result?: { command: string; session?: { id: string; phase?: string; model?: { id: string } } };
		error?: { code: string; message: string };
	}>;
	next(predicate: (message: ServerMessage) => boolean): Promise<ServerMessage>;
	close(): Promise<void>;
};

type ClientModule = { connectUnixTestClient(path: string): Promise<Client> };

function eventSnapshot(message: ServerMessage): { phase?: string } | undefined {
	if (message.type !== "event" || message.event.type !== "session_snapshot") return undefined;
	return message.event.snapshot as { phase?: string };
}

async function main(): Promise<void> {
	const socketPath = process.argv[2];
	const sourceWorktree = process.argv[3];
	if (socketPath === undefined || sourceWorktree === undefined) {
		throw new Error("conformance client requires a socket path and source worktree");
	}
	const clientModule = (await import(
		pathToFileURL(join(sourceWorktree, "packages/server/src/testing/client.ts")).href
	)) as ClientModule;
	const client = await clientModule.connectUnixTestClient(socketPath);
	try {
		const hello = await client.hello();
		assert.equal(hello.type, "hello");

		const created = await client.request({ command: "create", cwd: "/work", name: "Created" }, "create");
		assert.equal(created.ok, true, JSON.stringify(created));
		assert.equal(created.result?.command, "create");
		const sessionId = created.result?.session?.id;
		assert.ok(sessionId);

		const listed = await client.request({ command: "list" }, "list");
		assert.equal(listed.ok, true, JSON.stringify(listed));

		const attached = await client.request({ command: "attach", sessionId }, "attach");
		assert.equal(attached.ok, true, JSON.stringify(attached));
		assert.equal(attached.result?.session?.id, sessionId);

		const model = await client.request(
			{ command: "set_model", sessionId, model: { provider: "test", id: "large" } },
			"model",
		);
		assert.equal(model.ok, true, JSON.stringify(model));
		assert.equal(model.result?.session?.model?.id, "large");

		const thinking = await client.request(
			{ command: "set_thinking", sessionId, thinkingLevel: "high" },
			"thinking",
		);
		assert.equal(thinking.ok, true, JSON.stringify(thinking));

		const prompt = client.request({ command: "prompt", sessionId, text: "hello" }, "prompt");
		await client.next((message) => eventSnapshot(message)?.phase === "turn");
		const steer = await client.request({ command: "steer", sessionId, text: "adjust" }, "steer");
		assert.equal(steer.ok, true, JSON.stringify(steer));
		const abort = await client.request({ command: "abort", sessionId }, "abort");
		assert.equal(abort.ok, true, JSON.stringify(abort));
		const promptResponse = await prompt;
		assert.equal(promptResponse.ok, true, JSON.stringify(promptResponse));
		assert.equal(promptResponse.result?.session?.phase, "idle");

		const detached = await client.request({ command: "detach", sessionId }, "detach");
		assert.equal(detached.ok, true, JSON.stringify(detached));
		process.stdout.write(`${JSON.stringify({ ok: true })}\n`);
	} finally {
		await client.close();
	}
}

void main().catch((error: unknown) => {
	const message = error instanceof Error ? error.message : String(error);
	process.stdout.write(`${JSON.stringify({ ok: false, error: message })}\n`);
	process.exitCode = 1;
});
