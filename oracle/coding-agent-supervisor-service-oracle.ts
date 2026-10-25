import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case =
	| { kind: "prompt"; name: string; request: unknown }
	| { kind: "parse"; name: string; requestKind: string; response: unknown }
	| { kind: "instruction"; name: string; response: unknown }
	| { kind: "instructionContext"; name: string; instructions: string; timestamp: number }
	| { kind: "feedback"; name: string; reason: string }; 

void runOracle((api, request) => {
	const buildSupervisorPrompt = api.buildSupervisorPrompt as (request: unknown) => string;
	const parseSupervisorResponse = api.parseSupervisorResponse as (kind: string, response: unknown) => unknown;
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			if (input.kind === "prompt") {
				return { name: input.name, value: buildSupervisorPrompt(input.request) };
			}
			if (input.kind === "instruction") {
				const parse = api.parseSupervisorInstructionReviewDecision as (response: unknown) => unknown;
				return { name: input.name, value: parse(input.response) ?? null };
			}
			if (input.kind === "instructionContext") {
				const build = api.buildSupervisorInstructionReviewContext as (instructions: string, timestamp: number) => unknown;
				return { name: input.name, value: build(input.instructions, input.timestamp) };
			}
			if (input.kind === "feedback") {
				const feedback = api.supervisorInstructionRejectionFeedback as (reason: string) => string;
				return { name: input.name, value: feedback(input.reason) };
			}
			return {
				name: input.name,
				value: JSON.stringify(parseSupervisorResponse(input.requestKind, input.response) ?? null),
			};
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`supervisor-service oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
