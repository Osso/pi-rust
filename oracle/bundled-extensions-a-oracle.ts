// Differential oracle for the bundled `loop`, `codex-fast`, and `effort`
// extensions. Loads the real TypeScript modules from the pinned source tree and
// returns their registration metadata and handler outputs for concrete inputs.

import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type AnyRecord = Record<string, any>;

const extensionsDir = dirname(dirname(dirname(process.argv[2] ?? "")));

async function loadModule(relative: string): Promise<AnyRecord> {
	return (await import(
		pathToFileURL(join(extensionsDir, relative)).href
	)) as AnyRecord;
}

function notifyType(type: unknown): string | null {
	return typeof type === "string" ? type : null;
}

function makeModel(options: AnyRecord): AnyRecord {
	return {
		id: options.modelId ?? "test-model",
		name: "Test Model",
		api: options.api ?? "openai-codex-responses",
		provider: options.provider ?? "openai-codex",
		baseUrl: "https://example.test",
		reasoning: options.reasoning ?? true,
		thinkingLevelMap: options.thinkingLevelMap ?? {
			xhigh: "xhigh",
			max: "max",
			ultra: "max",
		},
		input: ["text", "image"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 200_000,
		maxTokens: 1000,
	};
}

function createLoopHarness(loopExtension: (pi: AnyRecord) => void): AnyRecord {
	let command: AnyRecord | undefined;
	let tool: AnyRecord | undefined;
	const notifications: Array<[string, string | null]> = [];
	const editor: string[] = [];
	const sends: unknown[] = [];
	const pi = {
		on() {},
		registerCommand(name: string, value: AnyRecord) {
			if (name === "loop") command = value;
		},
		registerTool(definition: AnyRecord) {
			if (definition.name === "loop") tool = definition;
		},
		sendUserMessage(...args: unknown[]) {
			sends.push(args);
		},
	};
	loopExtension(pi);
	const ctx = {
		cwd: "/repo",
		sessionManager: { getSessionId: () => "fixture-session" },
		isIdle: () => true,
		hasPendingMessages: () => false,
		ui: {
			notify(message: string, type?: string) {
				notifications.push([message, notifyType(type)]);
			},
			setEditorText(text: string) {
				editor.push(text);
			},
		},
	};
	return { command, tool, ctx, notifications, editor, sends };
}

function loopMetadata(harness: AnyRecord): unknown {
	return {
		toolName: harness.tool.name,
		toolLabel: harness.tool.label,
		description: harness.tool.description,
		promptSnippet: harness.tool.promptSnippet ?? null,
		promptGuidelines: harness.tool.promptGuidelines ?? null,
		approvalRequired: harness.tool.approvalRequired ?? null,
		parameters: harness.tool.parameters,
		commandDescription: harness.command.description ?? null,
	};
}

async function runLoopCase(
	loopExtension: (pi: AnyRecord) => void,
	input: AnyRecord,
): Promise<unknown> {
	const harness = createLoopHarness(loopExtension);
	if (typeof input.command === "string") {
		await harness.command.handler(input.command, harness.ctx);
		return {
			name: input.name,
			metadata: loopMetadata(harness),
			notifications: harness.notifications,
			editor: harness.editor,
			sends: harness.sends,
		};
	}
	const result = await harness.tool.execute(
		"loop-call",
		input.tool,
		undefined,
		undefined,
		harness.ctx,
	);
	return {
		name: input.name,
		metadata: loopMetadata(harness),
		text: result.content.map((item: AnyRecord) => item.text),
		details: result.details,
		notifications: harness.notifications,
		editor: harness.editor,
		sends: harness.sends,
	};
}

function createFastHarness(
	codexFastExtension: (pi: AnyRecord, options?: AnyRecord) => void,
	input: AnyRecord,
): AnyRecord {
	const authority = { serviceTier: input.tier ?? undefined };
	const handlers = new Map<
		string,
		(event: unknown, ctx: AnyRecord) => unknown
	>();
	const commands = new Map<string, AnyRecord>();
	const notifications: Array<[string, string | null]> = [];
	const statuses: Array<[string, string | null]> = [];
	const editor: string[] = [];
	const entries: unknown[] = [];
	const pi = {
		appendEntry(...args: unknown[]) {
			entries.push(args);
		},
		on(name: string, handler: (event: unknown, ctx: AnyRecord) => unknown) {
			handlers.set(name, handler);
		},
		registerCommand(name: string, value: AnyRecord) {
			commands.set(name, value);
		},
	};
	codexFastExtension(pi, { authority });
	let thinkingLevel = "off";
	const model = input.model === null ? undefined : makeModel(input.model ?? {});
	const ctx: AnyRecord = {
		model,
		multiAgentAgentId: input.child ? "child-agent" : undefined,
		sessionManager: {
			getBranch: () =>
				(input.branch ?? []).map((entry: AnyRecord) => ({
					type: "custom",
					...entry,
				})),
			isSubagentSession: () => false,
		},
		settingsManager: {
			getMergedSettings: () => input.settings ?? {},
		},
		ui: {
			notify(message: string, type?: string) {
				notifications.push([message, notifyType(type)]);
			},
			setEditorText(text: string) {
				editor.push(text);
			},
			setStatus(
				key: string,
				value: string | undefined | ((session?: unknown) => string | undefined),
			) {
				const displayed = typeof value === "function" ? value() : value;
				statuses.push([key, displayed ?? null]);
			},
		},
		getThinkingLevel: () => thinkingLevel,
		setThinkingLevel(level: string) {
			thinkingLevel = level;
		},
	};
	return {
		authority,
		handlers,
		commands,
		ctx,
		notifications,
		statuses,
		editor,
		entries,
	};
}

async function runFastCase(
	codexFastExtension: (pi: AnyRecord, options?: AnyRecord) => void,
	input: AnyRecord,
): Promise<unknown> {
	const harness = createFastHarness(codexFastExtension, input);
	let handlerResult: unknown;
	for (const op of input.ops as AnyRecord[]) {
		if (op.kind === "command") {
			await harness.commands.get("fast").handler(op.args, harness.ctx);
		} else if (op.kind === "session_start") {
			await harness.handlers.get("session_start")({}, harness.ctx);
		} else if (op.kind === "model_select") {
			const model = op.model === null ? undefined : makeModel(op.model ?? {});
			harness.ctx.model = model;
			await harness.handlers.get("model_select")({ model }, harness.ctx);
		} else if (op.kind === "before_provider_request") {
			const value = await harness.handlers.get("before_provider_request")(
				{ type: "before_provider_request", payload: op.payload },
				harness.ctx,
			);
			handlerResult = value === undefined ? null : value;
		}
	}
	return {
		name: input.name,
		commandDescription: harness.commands.get("fast")?.description ?? null,
		serviceTier: harness.authority.serviceTier ?? null,
		notifications: harness.notifications,
		statuses: harness.statuses,
		editor: harness.editor,
		entries: harness.entries,
		handlerResult: handlerResult ?? null,
	};
}

async function runEffortCase(
	effortExtension: (pi: AnyRecord) => void,
	themeModule: AnyRecord,
	input: AnyRecord,
): Promise<unknown> {
	const handlers = new Map<
		string,
		(event: unknown, ctx: AnyRecord) => unknown
	>();
	const commands = new Map<string, AnyRecord>();
	const notifications: Array<[string, string | null]> = [];
	const statuses: Array<[string, string | null]> = [];
	const editor: string[] = [];
	const entries: unknown[] = [];
	const selectCalls: Array<[string, string[]]> = [];
	let thinkingLevel = input.thinkingLevel ?? "off";
	let activeTools: string[] = [...(input.activeTools ?? [])];
	const toolUpdates: string[][] = []; 
	const pi = {
		getActiveTools: () => [...activeTools],
		setActiveTools(names: string[]) {
			activeTools = [...names];
			toolUpdates.push([...names]);
		},
		getThinkingLevel: () => thinkingLevel,
		setThinkingLevel(level: string) {
			thinkingLevel = level;
		},
		on(name: string, handler: (event: unknown, ctx: AnyRecord) => unknown) {
			handlers.set(name, handler);
		},
		registerCommand(name: string, value: AnyRecord) {
			commands.set(name, value);
		},
		appendEntry(...args: unknown[]) {
			entries.push(args);
		},
	};
	effortExtension(pi);
	const model = input.model === null ? undefined : makeModel(input.model ?? {});
	const ctx: AnyRecord = {
		model,
		multiAgentAgentId: input.child ? "child-agent" : undefined,
		sessionManager: {
			getBranch: () =>
				(input.branch ?? []).map((entry: AnyRecord) => ({
					type: "custom",
					...entry,
				})),
			isSubagentSession: () => false,
		},
		ui: {
			notify(message: string, type?: string) {
				notifications.push([message, notifyType(type)]);
			},
			async select(title: string, options: string[]) {
				selectCalls.push([title, options]);
				return input.selectResult;
			},
			setEditorText(text: string) {
				editor.push(text);
			},
			setStatus(key: string, value: string | undefined) {
				statuses.push([key, value ?? null]);
			},
			theme: themeModule.theme,
		},
		getThinkingLevel: () => thinkingLevel,
		setThinkingLevel(level: string) {
			thinkingLevel = level;
		},
	};
	let systemPrompt: string | null = null;
	for (const op of input.ops as AnyRecord[]) {
		if (op.kind === "command") {
			await commands.get(op.command).handler(op.args, ctx);
		} else if (op.kind === "session_start") {
			await handlers.get("session_start")(
				{ type: "session_start", reason: "resume" },
				ctx,
			);
		} else if (op.kind === "thinking_level_select") {
			await handlers.get("thinking_level_select")(
				{ level: op.level, previousLevel: op.previousLevel },
				ctx,
			);
		} else if (op.kind === "before_agent_start") {
			const result = await handlers.get("before_agent_start")(
				{ systemPrompt: op.systemPrompt },
				ctx,
			);
			systemPrompt = result?.systemPrompt ?? null;
		}
	}
	return {
		name: input.name,
		commandDescriptions: {
			effort: commands.get("effort")?.description ?? null,
			multiAgent: commands.get("multi-agent")?.description ?? null,
		},
		thinkingLevel,
		notifications,
		statuses,
		editor,
		entries,
		selectCalls,
		activeTools,
		toolUpdates,
		systemPrompt,
	};
}

void runOracle(async (api, request) => {
	const loopExtension = api.default as (pi: AnyRecord) => void;
	if (request.operation === "loop") {
		return {
			results: await Promise.all(
				requireCases(request).map((value) =>
					runLoopCase(loopExtension, value as AnyRecord),
				),
			),
		};
	}
	if (request.operation === "fast") {
		const module = await loadModule("codex-fast/src/index.ts");
		const codexFastExtension = module.default as (
			pi: AnyRecord,
			options?: AnyRecord,
		) => void;
		return {
			results: await Promise.all(
				requireCases(request).map((value) =>
					runFastCase(codexFastExtension, value as AnyRecord),
				),
			),
		};
	}
	if (request.operation === "effort") {
		const module = await loadModule("effort/src/index.ts");
		const effortExtension = module.default as (pi: AnyRecord) => void;
		const themePath = join(
			extensionsDir,
			"..",
			"src",
			"modes",
			"interactive",
			"theme",
			"theme.ts",
		);
		const themeModule = (await import(
			pathToFileURL(themePath).href
		)) as AnyRecord;
		const piTui = (await import("@earendil-works/pi-tui")) as AnyRecord;
		piTui.setCapabilities({ images: null, trueColor: true, hyperlinks: false });
		themeModule.initTheme(undefined, false);
		return {
			results: await Promise.all(
				requireCases(request).map((value) =>
					runEffortCase(effortExtension, themeModule, value as AnyRecord),
				),
			),
		};
	}
	throw new Error(`unknown operation ${request.operation}`);
}).catch((error: unknown) => {
	process.stderr.write(
		`bundled extensions oracle startup failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
