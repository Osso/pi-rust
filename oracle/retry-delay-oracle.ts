import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

function normalize(value: number): number | string {
	if (Number.isNaN(value)) return "NaN";
	if (Object.is(value, -0)) return "-0";
	if (!Number.isFinite(value)) return String(value);
	return value;
}

async function main(): Promise<void> {
	const source = process.argv[2];
	if (!source) throw new Error("retry-delay oracle requires a source module");
	const { calculateRetryDelayMs } = await import(pathToFileURL(source).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line) as { id: number; cases: string[][] };
		const output = request.cases.map((args) =>
			normalize(calculateRetryDelayMs(...args.map(Number))),
		);
		process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output })}\n`);
	}
}

void main().catch((error: unknown) => {
	console.error("retry-delay oracle failed:", error);
	process.exitCode = 1;
});
