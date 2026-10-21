import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import type { ModelsDevReasoningOption } from "@pi-oracle-source/ai/scripts/models-dev-reasoning-options.ts";

async function main(): Promise<void> {
	const sourcePath = process.argv[2];
	if (sourcePath === undefined)
		throw new Error("reasoning options oracle requires source module path");
	const api = await import(pathToFileURL(sourcePath).href);
	const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of input) {
		const options = JSON.parse(line) as ModelsDevReasoningOption[];
		const result = api.getEffortThinkingLevelMap(options);
		// Preserve undefined by omitted property, and compare defined map bytes verbatim.
		process.stdout.write(
			`${JSON.stringify({ json: JSON.stringify(result) })}\n`,
		);
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`reasoning options oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
