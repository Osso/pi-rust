import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Definition = {
	name: string;
	label: string;
	description: string;
	promptSnippet?: string;
	promptGuidelines?: string[];
	approvalRequired?: boolean;
	approvalKind?: string;
	parameters: unknown;
	executionMode?: string;
	renderShell?: string;
	renderCall?: unknown;
	renderResult?: unknown;
};

async function loadIndex(sourcePath: string): Promise<any> {
	return (await import(pathToFileURL(join(dirname(sourcePath), "index.ts")).href)) as any;
}

function describe(definition: Definition) {
	return {
		name: definition.name,
		label: definition.label,
		description: definition.description,
		promptSnippet: definition.promptSnippet ?? null,
		promptGuidelines: definition.promptGuidelines ?? null,
		approvalRequired: definition.approvalRequired ?? null,
		approvalKind: definition.approvalKind ?? null,
		parameters: definition.parameters,
		executionMode: definition.executionMode ?? null,
		renderShell: definition.renderShell ?? null,
		hasRenderCall: typeof definition.renderCall === "function",
		hasRenderResult: typeof definition.renderResult === "function",
	};
}

void runOracle(async (_module, request) => {
	const index = await loadIndex(process.argv[2] ?? "");
	const cwd = String(request.cwd);
	if (request.operation === "availability") {
		const savedPath = process.env.PATH;
		try {
			process.env.PATH = String(request.path);
			const options = request.injected ? { codeIndex: { operations: {} } } : undefined;
			return Object.keys(index.createToolDefinitionsForAvailableExecutables(cwd, options));
		} finally { process.env.PATH = savedPath; }
	}
	if (request.operation === "execute") {
		try { return { result: await index.createTool(request.name, cwd).execute("call", request.params) }; }
		catch (error) { return { error: (error as Error).message }; }
	}
	const names: string[] = index.DEFAULT_ACTIVE_TOOL_NAMES;
	const definitions = names.map((name) => describe(index.createToolDefinition(name, cwd)));
	const allDefinitions = index.createAllToolDefinitions(cwd);
	return {
		names,
		allNames: Array.from(index.allToolNames as Set<string>),
		definitions,
		coding: index.createCodingToolDefinitions(cwd).map((definition: Definition) => definition.name),
		readOnly: index.createReadOnlyToolDefinitions(cwd).map((definition: Definition) => definition.name),
		allKeys: Object.keys(allDefinitions),
	};
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent tools-index oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
