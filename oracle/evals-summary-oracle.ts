import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type HarnessObservation = {
	evalSet: string;
	groupKey: string;
	testName: string;
	file: string;
	harness: string;
	baseline: string;
	candidates: string[];
	repetition: number;
	totalTokens?: number;
	totalMs?: number;
	estimatedCostUsd?: number;
	outcome: "scored" | "unscored" | "skipped" | "pending" | "errored";
	score?: number;
};

type HarnessComparisonReport = {
	schemaVersion: 1;
	evalSets: Array<{ evalSet: string; comparisons: Array<Record<string, unknown>> }>;
	diagnostics: Array<Record<string, unknown>>;
};

type SummaryApi = {
	summarizeHarnessComparisons(observations: readonly HarnessObservation[]): HarnessComparisonReport;
	formatHarnessComparisonReport(report: HarnessComparisonReport): string;
};

type SummaryRequest = {
	id: RequestId;
	operation: "summarize";
	observations: HarnessObservation[];
};

type Response =
	| { id: RequestId; ok: true; output: { reportJson: string; formatted: string } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: SummaryRequest, api: SummaryApi): Response {
	try {
		const report = api.summarizeHarnessComparisons(request.observations);
		return {
			id: request.id,
			ok: true,
			output: {
				reportJson: JSON.stringify(report),
				formatted: api.formatHarnessComparisonReport(report),
			},
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: SummaryApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as SummaryRequest;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: SummaryApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) throw new Error("evals summary oracle requires a source module path");
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as SummaryApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`evals summary oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
