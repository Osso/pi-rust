import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

import { encodeServerMessage, PROTOCOL_VERSION } from "@earendil-works/pi-protocol";

type RequestId = string | number | null;

type ProtocolApi = {
	toProtocolModelMetadata(model: unknown, authenticated: boolean): unknown;
	toProtocolUserMessage(message: unknown, options: { id: string }): unknown;
	toProtocolAssistantMessage(message: unknown, options: { id: string }): unknown;
	toProtocolToolResultMessage(message: unknown, options: { id: string; call: unknown }): unknown;
	toProtocolUsage(usage: unknown): unknown;
	sanitizeProtocolDetails(value: unknown): unknown;
};

type Request =
	| { id: RequestId; operation: "model_metadata"; model: unknown; authenticated: boolean }
	| { id: RequestId; operation: "user_message"; itemId: string; message: unknown }
	| { id: RequestId; operation: "assistant_message"; itemId: string; message: unknown }
	| { id: RequestId; operation: "tool_result"; itemId: string; call: unknown; message: unknown }
	| { id: RequestId; operation: "usage"; usage: unknown }
	| { id: RequestId; operation: "sanitize_details"; value: unknown };

type Response =
	| { id: RequestId; ok: true; output: { frameHex: string } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function toHex(bytes: Uint8Array): string {
	return Buffer.from(bytes).toString("hex");
}

function snapshotFrame(item: unknown): string {
	const snapshot = {
		id: "session-1",
		cwd: "/workspace",
		createdAt: 1,
		updatedAt: 1,
		phase: "idle",
		model: { provider: "test-provider", id: "model-1" },
		thinkingLevel: "off",
		attached: true,
		locked: true,
		revision: 1,
		transcript: [item],
		queuedSteer: [],
		queuedSteerCount: 0,
	};
	return toHex(encodeServerMessage({ type: "event", event: { type: "session_snapshot", snapshot } }));
}

function execute(request: Request, api: ProtocolApi): string {
	switch (request.operation) {
		case "model_metadata": {
			const metadata = api.toProtocolModelMetadata(request.model, request.authenticated);
			const frame = encodeServerMessage({
				type: "hello",
				version: PROTOCOL_VERSION,
				connectionId: "connection-1",
				snapshot: {
					serverId: "server-1",
					protocolVersion: PROTOCOL_VERSION,
					revision: 0,
					sessions: [],
					models: [metadata],
				},
			});
			return toHex(frame);
		}
		case "user_message":
			return snapshotFrame(api.toProtocolUserMessage(request.message, { id: request.itemId }));
		case "assistant_message":
			return snapshotFrame(api.toProtocolAssistantMessage(request.message, { id: request.itemId }));
		case "tool_result":
			return snapshotFrame(
				api.toProtocolToolResultMessage(request.message, {
					id: request.itemId,
					call: request.call,
				}),
			);
		case "usage": {
			const usage = api.toProtocolUsage(request.usage);
			const frame = encodeServerMessage({
				type: "event",
				event: {
					type: "session_snapshot",
					snapshot: {
						id: "session-1",
						cwd: "/workspace",
						createdAt: 1,
						updatedAt: 1,
						phase: "idle",
						model: { provider: "test-provider", id: "model-1" },
						thinkingLevel: "off",
						attached: true,
						locked: true,
						revision: 1,
						transcript: [
							{
								id: "assistant-1",
								role: "assistant",
								content: [],
								model: { provider: "test-provider", id: "model-1" },
								status: "complete",
								stopReason: "stop",
								timestamp: 1,
								...(usage === undefined ? {} : { usage }),
							},
						],
						queuedSteer: [],
						queuedSteerCount: 0,
					},
				},
			});
			return toHex(frame);
		}
		case "sanitize_details": {
			const details = api.sanitizeProtocolDetails(request.value);
			const frame = encodeServerMessage({
				type: "event",
				event: {
					type: "session_snapshot",
					snapshot: {
						id: "session-1",
						cwd: "/workspace",
						createdAt: 1,
						updatedAt: 1,
						phase: "idle",
						model: { provider: "test-provider", id: "model-1" },
						thinkingLevel: "off",
						attached: true,
						locked: true,
						revision: 1,
						transcript: [
							{
								id: "tool-1",
								role: "tool",
								toolCallId: "call-1",
								toolName: "read",
								input: {},
								content: [],
								...(details === undefined ? {} : { details }),
								status: "complete",
								isError: false,
								timestamp: 1,
							},
						],
						queuedSteer: [],
						queuedSteerCount: 0,
					},
				},
			});
			return toHex(frame);
		}
	}
}

function main(api: ProtocolApi): void {
	const lines = createInterface({ input: process.stdin });
	lines.on("line", (line) => {
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const response: Response = { id, ok: true, output: { frameHex: execute(request, api) } };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		} catch (error) {
			const response: Response = { id, ok: false, error: normalizeError(error) };
			process.stdout.write(`${JSON.stringify(response)}\n`);
		}
	});
}

async function run(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) throw new Error("server protocol oracle requires a source module path");
	const api = (await import(pathToFileURL(sourceModulePath).href)) as ProtocolApi;
	main(api);
}

void run().catch((error: unknown) => {
	process.stderr.write(`server protocol oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
