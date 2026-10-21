import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type RetryCase = [string, string, string, string];
type RetryDelayApi = {
	calculateRetryDelayMs(attempt: number, base: number, cap: number, random: number): number;
};

function normalize(value: number): number | string {
	if (Number.isNaN(value)) return "NaN";
	if (Object.is(value, -0)) return "-0";
	if (!Number.isFinite(value)) return String(value);
	return value;
}

async function main(): Promise<void> {
	const source = process.argv[2];
	if (!source) throw new Error("retry-delay oracle requires a source module");
	const api = (await import(pathToFileURL(source).href)) as RetryDelayApi;
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line) as { id: number; cases: RetryCase[] };
		const output = request.cases.map(([attempt, base, cap, random]) =>
			normalize(api.calculateRetryDelayMs(Number(attempt), Number(base), Number(cap), Number(random))),
		);
		process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output })}\n`);
	}
}

void main().catch((error: unknown) => {
	console.error("retry-delay oracle failed:", error);
	process.exitCode = 1;
});
