import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case =
	| { kind: "prompt"; name: string; request: unknown }
	| { kind: "parse"; name: string; requestKind: string; response: unknown };

void runOracle((api, request) => {
	const buildSupervisorPrompt = api.buildSupervisorPrompt as (request: unknown) => string;
	const parseSupervisorResponse = api.parseSupervisorResponse as (kind: string, response: unknown) => unknown;
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			if (input.kind === "prompt") {
				return { name: input.name, value: buildSupervisorPrompt(input.request) };
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
