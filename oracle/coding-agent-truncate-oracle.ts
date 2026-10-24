import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Limit = number | "NaN" | "Infinity" | "-Infinity" | "-0";
type Options = { maxLines?: number; maxBytes?: number };
type WireOptions = { maxLines?: Limit; maxBytes?: Limit };
type Case =
	| { kind: "head" | "tail"; content: string; options?: WireOptions }
	| { kind: "line"; line: string; maxChars?: Limit };

type Api = {
	truncateHead(content: string, options?: Options): unknown;
	truncateTail(content: string, options?: Options): unknown;
	truncateLine(line: string, maxChars?: number): unknown;
};

function decodeLimit(value: Limit | undefined): number | undefined {
	return value === undefined ? undefined : Number(value);
}

void runOracle((module, request) => {
	const api = module as unknown as Api;
	return requireCases(request).map((value) => {
		const input = value as Case;
		if (input.kind === "line")
			return api.truncateLine(input.line, decodeLimit(input.maxChars));
		const truncate =
			input.kind === "head" ? api.truncateHead : api.truncateTail;
		const options =
			input.options == null
				? undefined
				: {
						maxLines: decodeLimit(input.options.maxLines),
						maxBytes: decodeLimit(input.options.maxBytes),
					};
		return truncate(input.content, options);
	});
}).catch((error: unknown) => {
	process.stderr.write(
		`coding-agent truncate oracle startup failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
