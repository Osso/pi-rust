import { createInterface } from "node:readline";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
async function main() {
	const root = process.argv[2];
	const { registerQuestionRelayTools } = await import(
		pathToFileURL(
			join(root, "extensions/agents-core/src/question-relay-tools.ts"),
		).href
	);
	const { MultiAgentStore } = await import(
		pathToFileURL(join(root, "src/core/multi-agent-store.ts")).href
	);
	const control = await import(
		pathToFileURL(join(root, "src/core/session-control-db.ts")).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const store = new MultiAgentStore();
		for (const agent of request.agents ?? [])
			store.publishLifecycleCoordinatorSnapshot(agent);
		const tools = new Map<string, any>();
		const sent: unknown[] = [];
		registerQuestionRelayTools(
			{ registerTool: (tool: any) => tools.set(tool.name, tool) },
			store,
			{
				contactParent: (params: unknown) => {
					sent.push(params);
					return {
						content: [],
						details: { message: { id: "queued", status: "pending" } },
					};
				},
			},
		);
		const signal = new AbortController();
		if (request.cancelled) signal.abort();
		const inboxRoot =
			request.answerBody === undefined
				? undefined
				: mkdtempSync(join(tmpdir(), "question-answer-"));
		const db = inboxRoot ? join(inboxRoot, "control.sqlite") : request.db;
		if (inboxRoot) {
			const recipient = {
				agentId: request.agentId,
				sessionId: "session-child",
			};
			control.registerRuntimeMailboxListener(db, recipient, process.pid);
			const messageId = `ask_questions:${request.agentId}:${request.id}`;
			control.enqueueStoredRuntimeMailboxMessage(db, {
				kind: "reply",
				message: {
					body: request.answerBody,
					id: messageId,
					kind: "reply",
					status: "pending",
					fromAgentId: "main",
					toAgentId: request.agentId,
				},
				recipient,
				sender: { agentId: null, sessionId: "parent" },
				storeRef: { sessionPath: "/store", messageId },
			});
		}
		const ctx = {
			multiAgentAgentId: request.agentId,
			multiAgentRequiresAgentId: request.requiresId,
			controlDbPath: db,
			sessionManager: { getSessionId: () => "session-child" },
		};
		try {
			const result = await tools
				.get(request.tool ?? "ask_questions")
				.execute(request.id, request.params, signal.signal, undefined, ctx);
			if (result.details?.runtimeMailboxDelivery) {
				result.details.runtimeMailboxDelivery.controlDbPath = "<db>";
				for (const message of result.details.runtimeMailboxDelivery.messages)
					message.storeRef.sessionPath = "<store>";
			}
			console.log(
				JSON.stringify({
					result: {
						content: result.content,
						details: result.details,
						isError: result.isError ?? null,
					},
					sent,
				}),
			);
		} catch (error) {
			console.log(
				JSON.stringify({
					error: error instanceof Error ? error.message : String(error),
					sent,
				}),
			);
		} finally {
			if (inboxRoot) rmSync(inboxRoot, { recursive: true, force: true });
		}
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
