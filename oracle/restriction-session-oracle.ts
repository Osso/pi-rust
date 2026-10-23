import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

const ORIGINAL = "In my own repository, list source filenames and explain the code; do not read credentials.";
const RESTRICTION = "Codex error: This content was flagged for possible cybersecurity risk. OpenAI request ID: req_500";
const TASK = "List source filenames only, without opening credentials or running commands.";
const PROPOSAL = { kind: "rescope", task: TASK, basisQuote: "list source filenames", reason: "A metadata-only subset of the authorized repository inspection." };

async function main(): Promise<void> {
	const source = pathToFileURL(process.argv[2]);
	const { createHarness } = await import(new URL("../../test/suite/harness.ts", source).href);
	const { fauxAssistantMessage, fauxToolCall } = await import(new URL("../../../ai/src/index.ts", source).href);
	serve(async (request) => {
		const calls: string[][] = [];
		const requests: string[] = [];
        const adviceSignals: Array<boolean | null> = [];
        let releaseReservedInput: (() => void) | undefined;
		const harness = await createHarness({
			fauxProvider: { api: "openai-codex-responses", provider: "faux" },
			models: [{ id: "faux-1", name: "Faux Model", reasoning: true, contextWindow: 100000, maxTokens: 4096 }],
			settings: { retry: { enabled: true, maxRetries: 3, baseDelayMs: 1 } },
			tools: [{ name: "echo", label: "echo", description: "Echo text", parameters: { type: "object", properties: { text: { type: "string" } } }, execute: async (_id: string, params: { text: string }) => ({ content: [{ type: "text", text: params.text }], details: undefined }) }],
			initialActiveToolNames: ["echo"],
			supervisorDecisionRequester: async (input: any) => {
				requests.push(input.kind);

                if (request.scenario === "adviceModel") harness.session.agent.state.model = { ...harness.session.agent.state.model };
                if (request.scenario === "adviceGoal") harness.sessionManager.setSessionGoalJson('{"objective":"new goal"}');
                if (request.scenario === "adviceRequest") harness.sessionManager.appendCustomEntry("codex_restriction_context", { requestId: "changed-request", userRequest: "Different authorization" });
                if (request.scenario === "adviceMessages") harness.session.agent.state.messages = harness.session.agent.state.messages.map((message: object) => ({ ...message }));
                if (request.scenario === "adviceMetadata") harness.sessionManager.appendCustomEntry("status", { message: "Still inspecting" });
                if (request.scenario === "adviceStatus") await harness.session.sendCustomMessage({ customType: "supervisor-status", content: "Still inspecting", display: true });
                if (request.scenario === "adviceReserveInput") releaseReservedInput = harness.session.reserveExternalUserInput();
                if (request.scenario === "adviceAbort") await harness.session.abort();
                if (request.scenario === "adviceReviewCancel") harness.session.cancelSupervisorReview();
                if (request.scenario === "adviceToolChange") harness.session.setActiveToolsByName([]);
                if (request.scenario === "adviceQueued") harness.session.agent.followUp({ role: "user", content: "new input", timestamp: 0 });
                adviceSignals.push(input.signal?.aborted ?? null);
				return { kind: "advisory", answer: JSON.stringify(request.scenario === "blocked" ? { kind: "blocked", reason: "No authorized subset" } : PROPOSAL) };
			},
		});
		harness.session.setActiveToolsByName(["echo"]);
        if (request.scenario === "modelDuringValidationStart") harness.session.subscribe((event: any) => {
            if (event.type === "message_start" && event.message.role === "custom" && event.message.customType === "supervisor_restriction_validation") {
                harness.session.agent.state.model = { ...harness.session.agent.state.model };
            }
        });
		try {
			const refusal = () => fauxAssistantMessage("", { stopReason: "error", errorMessage: RESTRICTION, timestamp: 0 });
			const validation = request.scenario === "malformed" ? fauxAssistantMessage("not JSON", { timestamp: 0 })
				: request.scenario === "validationTool" ? fauxAssistantMessage(fauxToolCall("echo", { text: "forbidden" }, { id: "validate-tool" }), { stopReason: "toolUse", timestamp: 0 })
				: fauxAssistantMessage(JSON.stringify({ allowed: request.scenario !== "denied", task: TASK, reason: "Repository metadata only.", basisQuote: PROPOSAL.basisQuote }), { timestamp: 0 });
			const operation = request.scenario === "repeatFlag" ? refusal()
				: request.scenario === "transient" ? fauxAssistantMessage("", { stopReason: "error", errorMessage: "overloaded_error", timestamp: 0 })
				: fauxAssistantMessage("Source names listed", { timestamp: 0 });
			harness.setResponses([refusal(), validation, operation, fauxAssistantMessage("Unexpected resend", { timestamp: 0 })].map(message => (context: any) => {
				calls.push((context.tools ?? []).map((tool: any) => tool.name));
				return message;
			}));
			await harness.session.prompt(ORIGINAL);
			return {
				requests, adviceSignals, calls, pending: harness.getPendingResponseCount(), activeTools: harness.session.getActiveToolNames(),
				model: harness.session.model.id,
				ends: harness.eventsOfType("agent_end").map((event: any) => ({ willRetry: event.willRetry, continuation: event.sessionContinuation ?? null })),
				autoRetryStarts: harness.eventsOfType("auto_retry_start").length,
                userContents: harness.session.messages.filter((message: any) => message.role === "user").map((message: any) => message.content),
				assistantContents: harness.session.messages.filter((message: any) => message.role === "assistant").map((message: any) => message.content),
				customContents: harness.session.messages.filter((message: any) => message.role === "custom").map((message: any) => ({ type: message.customType, content: message.content })),
				toolResults: harness.session.messages.filter((message: any) => message.role === "toolResult").map((message: any) => ({ name: message.toolName, content: message.content, isError: message.isError })),
			};
		} finally { releaseReservedInput?.(); harness.cleanup(); }
	});
}
void main();
