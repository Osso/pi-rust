import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type ToolFixture = {
	name: string;
	label?: string;
	description: string;
	parameters: unknown;
	executionMode?: string;
};

type ContextFixture = {
	systemPrompt?: string;
	messages: unknown[];
	tools: ToolFixture[];
};

function agentContext(context: ContextFixture) {
	return {
		...context,
		tools: context.tools.map((tool) => ({
			name: tool.name,
			label: tool.label,
			description: tool.description,
			parameters: tool.parameters,
			prepareArguments: (args: unknown) => args,
			prepareExecution: () => undefined,
			executionMode: tool.executionMode,
			execute: async () => ({ content: [] }),
		})),
	};
}

async function main(): Promise<void> {
	const directory = process.argv[2];
	const faux = await import(
		pathToFileURL(join(directory, "providers/faux.ts")).href
	);
	const estimate = await import(
		pathToFileURL(join(directory, "utils/estimate.ts")).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const context = agentContext(JSON.parse(line).context as ContextFixture);
		const core = faux.createFauxCore({
			api: "test-api",
			provider: "test-provider",
			models: [],
			tokenSize: { min: 1, max: 1 },
		});
		core.setResponses([faux.fauxAssistantMessage("reply", { timestamp: 0 })]);
		const result = await core
			.streamSimple(core.getModel(""), context, {})
			.result();
		process.stdout.write(
			`${JSON.stringify({ toolWire: JSON.stringify(context.tools), estimate: estimate.estimateContextTokens(context), input: result.usage.input })}\n`,
		);
	}
}

void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
