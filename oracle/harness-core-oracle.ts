import { mkdirSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type HarnessEvent = Record<string, unknown>;

type ScriptOperation =
	| { op: "on"; tag: string; type: "run_start" | "run_end"; thenEmit?: HarnessEvent }
	| { op: "off"; tag: string }
	| { op: "watch"; tag: string; snapshot: unknown; emitDuringSnapshot?: HarnessEvent }
	| { op: "start"; tag: string }
	| { op: "unsubscribe"; tag: string }
	| { op: "emit"; event: HarnessEvent };

type Request = {
	id: RequestId;
	operation: string;
	script?: ScriptOperation[];
	messages?: unknown[];
	bash?: Record<string, unknown>;
	customMessage?: {
		customType: string;
		content: unknown;
		display: boolean;
		details?: unknown;
		timestamp: number | string;
	};
	fixture?: Fixture;
};

type Fixture = {
	root: string;
	files?: Record<string, string>;
	symlinks?: Array<{ target: string; path: string }>;
	dirs?: string[];
	inputs?: Array<{ path: string; source: unknown }>;
	paths?: string[];
};

type NodeExecutionEnvApi = {
	NodeExecutionEnv: new (options: { cwd: string }) => unknown;
};

type SkillsApi = {
	loadSkills(env: unknown, dirs: string | string[]): Promise<{ skills: unknown[]; diagnostics: unknown[] }>;
	loadSourcedSkills(
		env: unknown,
		inputs: Array<{ path: string; source: unknown }>,
	): Promise<{ skills: unknown[]; diagnostics: unknown[] }>;
};

type PromptTemplatesApi = {
	loadPromptTemplates(
		env: unknown,
		paths: string | string[],
	): Promise<{ promptTemplates: unknown[]; diagnostics: unknown[] }>;
	loadSourcedPromptTemplates(
		env: unknown,
		inputs: Array<{ path: string; source: unknown }>,
	): Promise<{ promptTemplates: unknown[]; diagnostics: unknown[] }>;
};

type EventsApi = {
	HarnessEventBus: new () => {
		on(type: string, listener: (event: HarnessEvent) => void): () => void;
		emit(event: HarnessEvent): void;
		watch<TSnapshot>(captureSnapshot: () => TSnapshot): WatchHandle;
	};
};

type WatchHandle = {
	snapshot: unknown;
	start(listener: (event: HarnessEvent) => void): void;
	unsubscribe(): void;
};

type MessagesApi = {
	bashExecutionToText(message: Record<string, unknown>): string;
	convertToLlm(messages: unknown[]): unknown[];
	createCustomMessage(
		customType: string,
		content: unknown,
		display: boolean,
		details: unknown,
		timestamp: number | string,
	): unknown;
};

type Api = {
	events: EventsApi;
	messages: MessagesApi;
	skills: SkillsApi;
	promptTemplates: PromptTemplatesApi;
	nodeEnv: NodeExecutionEnvApi;
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function executeEventScript(api: EventsApi, script: ScriptOperation[]): unknown {
	const bus = new api.HarnessEventBus();
	const direct = new Map<string, HarnessEvent[]>();
	const watched = new Map<string, HarnessEvent[]>();
	const snapshots = new Map<string, unknown>();
	const unsubscribes = new Map<string, () => void>();
	const watches = new Map<string, WatchHandle>();

	for (const operation of script) {
		switch (operation.op) {
			case "on": {
				const events: HarnessEvent[] = [];
				direct.set(operation.tag, events);
				unsubscribes.set(
					operation.tag,
					bus.on(operation.type, (event) => {
						events.push(event);
						if (operation.thenEmit) bus.emit(operation.thenEmit);
					}),
				);
				break;
			}
			case "off":
				unsubscribes.get(operation.tag)?.();
				break;
			case "watch": {
				const events: HarnessEvent[] = [];
				watched.set(operation.tag, events);
				const watch = bus.watch(() => {
					if (operation.emitDuringSnapshot) bus.emit(operation.emitDuringSnapshot);
					return operation.snapshot;
				});
				snapshots.set(operation.tag, watch.snapshot);
				watches.set(operation.tag, watch);
				break;
			}
			case "start": {
				const events = watched.get(operation.tag);
				watches.get(operation.tag)?.start((event) => events?.push(event));
				break;
			}
			case "unsubscribe":
				watches.get(operation.tag)?.unsubscribe();
				break;
			case "emit":
				bus.emit(operation.event);
				break;
		}
	}

	return {
		direct: Object.fromEntries(direct),
		watched: Object.fromEntries(watched),
		snapshots: Object.fromEntries(snapshots),
	};
}

function createFixture(fixture: Fixture, api: Api): unknown {
	for (const [relativePath, content] of Object.entries(fixture.files ?? {})) {
		const fullPath = join(fixture.root, relativePath);
		mkdirSync(dirname(fullPath), { recursive: true });
		writeFileSync(fullPath, content);
	}
	for (const directory of fixture.dirs ?? []) mkdirSync(join(fixture.root, directory), { recursive: true });
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
		return Object.fromEntries(
			Object.entries(value).map(([key, item]) => [key, normalizeValue(item, root)]),
		);
	}
	return value;
}

async function executeFixtureOperation(request: Request, api: Api): Promise<Response> {
	const fixture = request.fixture;
	if (!fixture) throw new Error(`${request.operation} requires fixture`);
	const env = createFixture(fixture, api);
	switch (request.operation) {
		case "skills_load": {
			const result = await api.skills.loadSkills(env, fixture.paths ?? []);
			return { id: request.id, ok: true, output: normalizeValue(result, fixture.root) };
		}
		case "skills_load_sourced": {
			const result = await api.skills.loadSourcedSkills(env, fixture.inputs ?? []);
			return { id: request.id, ok: true, output: normalizeValue(result, fixture.root) };
		}
		case "prompt_templates_load": {
			const result = await api.promptTemplates.loadPromptTemplates(env, fixture.paths ?? []);
			return { id: request.id, ok: true, output: normalizeValue(result, fixture.root) };
		}
		case "prompt_templates_load_sourced": {
			const result = await api.promptTemplates.loadSourcedPromptTemplates(env, fixture.inputs ?? []);
			return { id: request.id, ok: true, output: normalizeValue(result, fixture.root) };
		}
		default:
			throw new Error(`unsupported operation: ${request.operation}`);
	}
}

async function execute(request: Request, api: Api): Promise<Response> {
	if (request.operation.startsWith("skills_") || request.operation.startsWith("prompt_templates_")) {
		return executeFixtureOperation(request, api);
	}
	switch (request.operation) {
		case "event_bus":
			return { id: request.id, ok: true, output: executeEventScript(api.events, request.script ?? []) };
		case "bash_execution_to_text":
			return { id: request.id, ok: true, output: api.messages.bashExecutionToText(request.bash ?? {}) };
		case "convert_to_llm":
			return { id: request.id, ok: true, output: api.messages.convertToLlm(request.messages ?? []) };
		case "create_custom_message": {
			const message = request.customMessage;
			if (!message) throw new Error("create_custom_message requires customMessage");
			return {
				id: request.id,
				ok: true,
				output: api.messages.createCustomMessage(
					message.customType,
					message.content,
					message.display,
					message.details,
					message.timestamp,
				),
			};
		}
		default:
			throw new Error(`unsupported operation: ${request.operation}`);
	}
}

async function executeRequestLine(line: string, api: Api): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(await execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: Api): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let queue = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(() => executeRequestLine(line, api));
		}
	});
}

function readSourceModules(): string[] {
	const modules = process.argv.slice(2);
	if (modules.length === 0) throw new Error("harness core oracle requires source module paths");
	return modules;
}

async function main(): Promise<void> {
	const [eventsPath, messagesPath, skillsPath, promptTemplatesPath, nodeEnvPath] = readSourceModules();
	if (!eventsPath || !messagesPath || !skillsPath || !promptTemplatesPath || !nodeEnvPath) {
		throw new Error("harness core oracle requires events, messages, skills, prompt-templates and node-env modules");
	}
	const events = (await import(pathToFileURL(eventsPath).href)) as EventsApi;
	const messages = (await import(pathToFileURL(messagesPath).href)) as MessagesApi;
	const skills = (await import(pathToFileURL(skillsPath).href)) as SkillsApi;
	const promptTemplates = (await import(pathToFileURL(promptTemplatesPath).href)) as PromptTemplatesApi;
	const nodeEnv = (await import(pathToFileURL(nodeEnvPath).href)) as NodeExecutionEnvApi;
	serveRequests({ events, messages, skills, promptTemplates, nodeEnv });
}

void main().catch((error: unknown) => {
	process.stderr.write(`harness core oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
