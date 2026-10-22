import { mkdirSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Fixture = {
	root: string;
	files?: Record<string, string>;
	symlinks?: Array<{ target: string; path: string }>;
	dirs?: string[];
	inputs?: Array<{ path: string; source: unknown }>;
	paths?: string[];
};
type Request = {
	id: number | string | null;
	operation: string;
	fixture?: Fixture;
	messages?: unknown[];
	argsString?: number[];
	contentUnits?: number[];
	bash?: unknown;
	customMessage?: { customType: string; content: unknown; display: boolean; details: unknown; timestamp: string };
};
type Api = {
	messages: {
		bashExecutionToText(message: unknown): string;
		convertToLlm(messages: unknown[]): unknown[];
		createCustomMessage(type: string, content: unknown, display: boolean, details: unknown, timestamp: string): unknown;
	};
	skills: {
		loadSkills(env: unknown, paths: string[]): Promise<unknown>;
		loadSourcedSkills(env: unknown, inputs: unknown[]): Promise<unknown>;
	};
	promptTemplates: {
		parseCommandArgs(input: string): string[];
		substituteArgs(content: string, args: string[]): string;
		loadPromptTemplates(env: unknown, paths: string[]): Promise<unknown>;
		loadSourcedPromptTemplates(env: unknown, inputs: unknown[]): Promise<unknown>;
	};
	nodeEnv: { NodeExecutionEnv: new (options: { cwd: string }) => unknown };
};

function createFixture(fixture: Fixture, api: Api): unknown {
	for (const [path, content] of Object.entries(fixture.files ?? {})) {
		const fullPath = join(fixture.root, path);
		mkdirSync(dirname(fullPath), { recursive: true });
		writeFileSync(fullPath, content);
	}
	for (const dir of fixture.dirs ?? []) mkdirSync(join(fixture.root, dir), { recursive: true });
	for (const link of fixture.symlinks ?? []) {
		mkdirSync(dirname(join(fixture.root, link.path)), { recursive: true });
		symlinkSync(join(fixture.root, link.target), join(fixture.root, link.path));
	}
	return new api.nodeEnv.NodeExecutionEnv({ cwd: fixture.root });
}

function normalizeValue(value: unknown, root: string): unknown {
	if (typeof value === "string") return value.split(root).join("<root>");
	if (Array.isArray(value)) return value.map((item) => normalizeValue(item, root));
	if (value && typeof value === "object") {
		return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, normalizeValue(item, root)]));
	}
	return value;
}

async function executeFixture(request: Request, api: Api): Promise<unknown> {
	const fixture = request.fixture;
	if (!fixture) throw new Error(`${request.operation} requires fixture`);
	const env = createFixture(fixture, api);
	let result: unknown;
	switch (request.operation) {
		case "skills_load": result = await api.skills.loadSkills(env, fixture.paths ?? []); break;
		case "skills_load_sourced": result = await api.skills.loadSourcedSkills(env, fixture.inputs ?? []); break;
		case "prompt_templates_load": result = await api.promptTemplates.loadPromptTemplates(env, fixture.paths ?? []); break;
		case "prompt_templates_load_sourced": result = await api.promptTemplates.loadSourcedPromptTemplates(env, fixture.inputs ?? []); break;
		default: throw new Error(`unsupported operation: ${request.operation}`);
	}
	return normalizeValue(result, fixture.root);
}

async function execute(request: Request, api: Api): Promise<unknown> {
	if (request.operation === "argument_units") {
		const args = api.promptTemplates.parseCommandArgs(String.fromCharCode(...(request.argsString ?? [])));
		const output = api.promptTemplates.substituteArgs(String.fromCharCode(...(request.contentUnits ?? [])), args);
		return { args: args.map(codeUnits), output: codeUnits(output) };
	}
	if (request.operation === "prompt_template_units") {
		const result = await executeFixture({ ...request, operation: "prompt_templates_load" }, api) as { promptTemplates: Array<{ description: string }> };
		return result.promptTemplates.map((template) => Array.from({ length: template.description.length }, (_, index) => template.description.charCodeAt(index)));
	}
	if (request.operation.startsWith("skills_") || request.operation.startsWith("prompt_templates_")) {
		return executeFixture(request, api);
	}
	switch (request.operation) {
		case "bash_execution_to_text": return api.messages.bashExecutionToText(request.bash ?? {});
		case "convert_to_llm": return api.messages.convertToLlm(request.messages ?? []);
		case "create_custom_message": {
			const m = request.customMessage;
			if (!m) throw new Error("create_custom_message requires customMessage");
			return api.messages.createCustomMessage(m.customType, m.content, m.display, m.details, m.timestamp);
		}
		default: throw new Error(`unsupported operation: ${request.operation}`);
	}
}

function codeUnits(value: string): number[] {
	return Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
}

async function main(): Promise<void> {
	const paths = process.argv.slice(2);
	if (paths.length !== 4) throw new Error("requires messages, skills, prompt templates and node env modules");
	const [messages, skills, promptTemplates, nodeEnv] = await Promise.all(paths.map((path) => import(pathToFileURL(path).href)));
	const api = { messages, skills, promptTemplates, nodeEnv } as Api;
	for await (const line of createInterface({ input: process.stdin })) {
		let id: Request["id"] = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			process.stdout.write(`${JSON.stringify({ id, ok: true, output: await execute(request, api) })}\n`);
		} catch (error) {
			const normalized = error instanceof Error ? { name: error.name, message: error.message } : { name: "UnknownError", message: String(error) };
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalized })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`harness core oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
