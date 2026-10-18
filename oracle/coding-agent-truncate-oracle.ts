import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Options = { maxLines?: number; maxBytes?: number };
type Case =
	| { kind: "head" | "tail"; content: string; options?: Options }
	| { kind: "line"; line: string; maxChars?: number };

type Api = {
	truncateHead(content: string, options?: Options): unknown;
	truncateTail(content: string, options?: Options): unknown;
	truncateLine(line: string, maxChars?: number): unknown;
};

void runOracle((module, request) => {
	const api = module as unknown as Api;
	return requireCases(request).map((value) => {
		const input = value as Case;
		if (input.kind === "line") return api.truncateLine(input.line, input.maxChars);
		const truncate = input.kind === "head" ? api.truncateHead : api.truncateTail;
		return truncate(input.content, input.options);
	});
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent truncate oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
