import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type NumericCase = {
	stat: string | null;
	offset?: string;
	limit?: string;
	emptyRange?: boolean;
	hasRange?: boolean;
	mime?: string;
};
type ReadTool = { execute(id: string, params: Record<string, unknown>): Promise<unknown> };
type ReadApi = { createReadTool(cwd: string, options: { operations: object }): ReadTool };

void runOracle(async (module, request) => {
	const api = module as unknown as ReadApi;
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as NumericCase;
		const trace: unknown[] = [];
		const operations = {
			access: async () => { trace.push(["access"]); },
			...(input.mime === undefined ? {} : { detectImageMimeType: async () => { trace.push(["mime"]); return input.mime; } }),
			readFile: async () => { trace.push(["read"]); return Buffer.from("one\ntwo\nthree"); },
			...(input.stat === null ? {} : { stat: async () => { trace.push(["stat"]); return { size: Number(input.stat) }; } }),
			...(input.hasRange === false ? {} : { readRange: async (_path: string, start: number, end: number) => {
				trace.push(["range", start, end]);
				return Buffer.from(input.emptyRange ? "" : "head\n");
			} }),
		};
		const params: Record<string, unknown> = { path: "virtual.txt" };
		if (input.offset !== undefined) params.offset = Number(input.offset);
		if (input.limit !== undefined) params.limit = Number(input.limit);
		try { output.push({ result: await api.createReadTool(String(request.cwd), { operations }).execute("call", params), trace }); }
		catch (error) { output.push({ error: (error as Error).message, trace }); }
	}
	return JSON.parse(JSON.stringify(output, (_key, value) =>
		typeof value === "number" && !Number.isFinite(value) ? { $number: String(value) } : value,
	));
}).catch((error: unknown) => { console.error(error); process.exitCode = 1; });
