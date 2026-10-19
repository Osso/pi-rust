import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type ObservationInput = {
	kind: "observation";
	name: string;
	reason: "architect_request" | "session_state_changed";
	requests: unknown[];
	sessions: unknown[];
};

void runOracle((api, request) => {
	const buildArchitectPrompt = api.buildArchitectPrompt as (observation: unknown) => string;
	const systemPrompt = api.ARCHITECT_SYSTEM_PROMPT as string;
	return {
		results: requireCases(request).map((value) => {
			if ((value as { kind: string }).kind === "system") {
				return { name: "system", value: systemPrompt };
			}
			const input = value as ObservationInput;
			// Same object-literal order as createArchitectObservation.
			const observation = { reason: input.reason, requests: input.requests, sessions: input.sessions };
			return { name: input.name, value: buildArchitectPrompt(observation) };
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`architect-prompt oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
