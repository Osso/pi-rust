import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const { model, context, options, apiKey, baseMaxTokens, modelMaxTokens, level, budgets } = request;
		const output = request.operation === "base"
			? api.buildBaseOptions(model, context, options, apiKey)
			: request.operation === "budget"
				? api.adjustMaxTokensForThinking(baseMaxTokens, modelMaxTokens, level, budgets)
				: { reasoning: api.clampReasoning(level) ?? null, standard: api.clampStandardReasoningLevel(level) ?? null };
		process.stdout.write(`${JSON.stringify(output)}\n`);
	}
}

void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
