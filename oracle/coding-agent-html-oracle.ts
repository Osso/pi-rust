import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case =
	| { kind: "entity"; name: string; input: string }
	| { kind: "at"; name: string; input: string; index: number };

void runOracle((api, request) => {
	const decodeHtmlEntity = api.decodeHtmlEntity as (entity: string) => string | undefined;
	const decodeHtmlEntityAt = api.decodeHtmlEntityAt as (
		html: string,
		index: number,
	) => { text: string; length: number } | undefined;
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			if (input.kind === "entity") {
				return { name: input.name, value: decodeHtmlEntity(input.input) ?? null };
			}
			return { name: input.name, value: decodeHtmlEntityAt(input.input, input.index) ?? null };
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`html oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
