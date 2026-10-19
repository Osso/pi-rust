import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case =
	| { kind: "edit"; cwd: string; params: Record<string, unknown> }
	| { kind: "write"; cwd: string; params: Record<string, unknown> }
	| { kind: "prepare"; cwd: string; params: unknown };

type Definition = { prepareArguments?: (input: unknown) => unknown };

type Tool = { execute(id: string, params: unknown): Promise<unknown> };

type Api = {
	createEditTool(cwd: string): Tool;
	createWriteTool(cwd: string): Tool;
	createEditToolDefinition(cwd: string): Definition;
};

async function loadSibling(): Promise<Api> {
	const sourcePath = process.argv[2];
	if (sourcePath === undefined) throw new Error("edit/write oracle requires a source module path");
	const editModule = (await import(pathToFileURL(sourcePath).href)) as Api;
	const writeModule = (await import(pathToFileURL(join(dirname(sourcePath), "write.ts")).href)) as Api;
	return {
		createEditTool: editModule.createEditTool,
		createWriteTool: writeModule.createWriteTool,
		createEditToolDefinition: editModule.createEditToolDefinition,
	};
}

const sibling = loadSibling();

void runOracle(async (_module, request) => {
	const api = await sibling;
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		try {
			if (input.kind === "prepare") {
				const definition = api.createEditToolDefinition(input.cwd);
				results.push({ ok: true, value: definition.prepareArguments?.(input.params) });
				continue;
			}
			const tool = input.kind === "edit" ? api.createEditTool(input.cwd) : api.createWriteTool(input.cwd);
			const result = await tool.execute(`${input.kind}-differential`, input.params);
			results.push({ ok: true, value: result });
		} catch (error) {
			results.push({ ok: false, error: error instanceof Error ? error.message : String(error) });
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent edit/write-tool oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
