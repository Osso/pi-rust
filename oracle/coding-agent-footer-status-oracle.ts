import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case =
	| { kind: "sanitize"; name: string; input: string }
	| { kind: "lines"; name: string; statuses: Array<[string, string]> };

void runOracle((api, request) => {
	const sanitizeFooterStatusText = api.sanitizeFooterStatusText as (value: string) => string;
	const formatExtensionStatusTextLines = api.formatExtensionStatusTextLines as (
		statuses: ReadonlyMap<string, string>,
	) => string[];
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			if (input.kind === "sanitize") {
				return { name: input.name, value: sanitizeFooterStatusText(input.input) };
			}
			return { name: input.name, value: formatExtensionStatusTextLines(new Map(input.statuses)) };
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`footer-status oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
