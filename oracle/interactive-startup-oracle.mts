import { mkdirSync, readFileSync, writeFileSync, writeSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import type { Model } from "@pi-oracle-source/ai/src/types.ts";
import type { CreateAgentSessionRuntimeFactory } from "@pi-oracle-source/coding-agent/src/core/agent-session-runtime.ts";
import type { ExtensionAPI } from "@pi-oracle-source/coding-agent/src/core/extensions/types.ts";
import type { Terminal } from "@pi-oracle-source/tui/src/terminal.ts";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const load = (path: string) => import(pathToFileURL(`${source}/${path}`).href);
const { createAgentSession } = await load("core/sdk.ts");
const { AgentSessionRuntime } = await load("core/agent-session-runtime.ts");
const { SessionManager } = await load("core/session-manager.ts");
const { SettingsManager } = await load("core/settings-manager.ts");
const { AuthStorage } = await load("core/auth-storage.ts");
const { ModelRegistry } = await load("core/model-registry.ts");
const { DefaultResourceLoader } = await load("core/resource-loader.ts");
const { InteractiveMode } = await load("modes/interactive/interactive-mode.ts");
const themes = await load("modes/interactive/theme/theme.ts");
const { Text } = await import(
	pathToFileURL(`${source}/../../tui/src/index.ts`).href
);
const units = (text: string) =>
	Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));

class CaptureTerminal implements Terminal {
	columns = 120;
	rows = 40;
	kittyProtocolActive = false;
	titles: string[] = [];
	writes: string[] = [];
	input?: (text: string) => void;
	start(onInput: (text: string) => void) {
		this.input = onInput;
	}
	stop() {}
	async drainInput() {}
	write(text: string) {
		this.writes.push(text);
	}
	moveBy() {}
	hideCursor() {}
	showCursor() {}
	clearLine() {}
	clearFromCursor() {}
	clearScreen() {}
	setTitle(title: string) {
		this.titles.push(title);
	}
	setProgress() {}
}

interface Request {
	cwd: string;
	agentDir: string;
	skillPath: string;
	models: Model<"faux">[];
	quiet: boolean;
	verbose: boolean;
	scoped: boolean;
	restored: boolean;
	watchChanges: boolean;
	colors: string[];
	importFailure?:
		| "missing"
		| "factory"
		| "cancel"
		| "hook-cancel"
		| "cwd-cancel"
		| "cwd-retry-error"
		| "success"
		| "cwd-success"
		| "cwd-retry-cancel"
		| "memory"
		| "usage"
		| "factory-string"
		| "factory-object";
	importPath?: string;
}

async function captureStartup(request: Request) {
	const { cwd, agentDir, skillPath } = request;
	mkdirSync(agentDir, { recursive: true });
	const settingsManager = SettingsManager.inMemory({
		quietStartup: request.quiet,
	});
	const extension = (api: ExtensionAPI) => {
		api.on("resources_discover", () => ({ skillPaths: [skillPath] }));
		if (
			request.importFailure === "hook-cancel" ||
			request.importFailure === "cwd-retry-cancel"
		) {
			let attempts = 0;
			api.on("session_before_switch", () => {
				attempts++;
				return request.importFailure === "hook-cancel" || attempts === 2
					? { cancel: true }
					: undefined;
			});
		}
	};
	const loader = new DefaultResourceLoader({
		cwd,
		agentDir,
		settingsManager,
		extensionFactories: [extension],
		noExtensions: true,
		noSkills: true,
		noPromptTemplates: true,
		noThemes: true,
		noContextFiles: true,
	});
	await loader.reload();
	const authStorage = AuthStorage.inMemory({
		faux: { type: "api_key", key: "faux-key" },
	});
	const modelRegistry = new ModelRegistry(authStorage);
	modelRegistry.registerProvider("faux", {
		api: "faux",
		baseUrl: "http://localhost:0",
		apiKey: "fixture",
		models: request.models,
	});
	const sessionManager =
		request.importFailure && request.importFailure !== "memory"
			? SessionManager.create(cwd, join(cwd, "sessions"))
			: SessionManager.inMemory(cwd);
	if (request.restored)
		sessionManager.appendMessage({
			role: "user",
			content: "Restored startup message",
			timestamp: 0,
		});
	const scopedModels = request.scoped
		? request.models.map((model, index) => ({
				model,
				thinkingLevel: index === 0 ? "off" : "medium",
			}))
		: [];
	const result = await createAgentSession({
		cwd,
		agentDir,
		authStorage,
		modelRegistry,
		resourceLoader: loader,
		settingsManager,
		sessionManager,
		model: request.models[0],
		thinkingLevel: "off",
		scopedModels,
	});
	const services = {
		cwd,
		agentDir,
		authStorage,
		modelRegistry,
		resourceLoader: loader,
		settingsManager,
		diagnostics: [],
	};
	const factory: CreateAgentSessionRuntimeFactory = async (options) => {
		if (!request.importFailure)
			throw new Error("startup fixture never replaces session");
		if (
			request.importFailure === "factory" ||
			request.importFailure === "cwd-retry-error"
		)
			throw new Error("File not found: imported dependency");
		if (request.importFailure === "factory-string")
			return Promise.reject("import failure \ud800");
		if (request.importFailure === "factory-object")
			return Promise.reject({ reason: "bad import" });
		const next = await createAgentSession({
			...services,
			cwd: options.cwd,
			sessionManager: options.sessionManager,
			model: request.models[0],
			thinkingLevel: "off",
			sessionStartEvent: options.sessionStartEvent,
		});
		return {
			...next,
			services: { ...services, cwd: options.cwd },
			diagnostics: [],
		};
	};
	const runtime = new AgentSessionRuntime(result.session, services, factory);
	themes.initTheme("dark", false);
	const mode = new InteractiveMode(runtime, { verbose: request.verbose });
	const terminal = new CaptureTerminal();
	mode.ui.terminal = terminal;
	const consoleLines: string[] = [];
	const originalLog = console.log;
	console.log = (message: string) => {
		consoleLines.push(message);
	};
	try {
		await mode.init();
		if (request.importFailure)
			await captureImportFailure(mode, runtime, terminal, request);
		const render = (container: { render(width: number): string[] }) =>
			container.render(120).map(units);
		const output = {
			header: render(mode.headerContainer),
			resources: render(mode.loadedResourcesContainer),
			chat: render(mode.chatContainer),
			count: mode.footerDataProvider.getAvailableProviderCount(),
			titles: terminal.titles.map(units),
			console: consoleLines.map(units),
			skills: loader
				.getSkills()
				.skills.map((skill: { name: string }) => skill.name),
		};
		if (request.watchChanges) {
			Object.assign(output, {
				changes: await captureChanges(mode, terminal, request),
			});
		}
		return output;
	} finally {
		console.log = originalLog;
		mode.stop();
		result.session.dispose();
	}
}

async function captureImportFailure(
	mode: {
		chatContainer: { render(width: number): string[] };
		statusContainer: {
			render(width: number): string[];
			addChild(child: unknown): void;
		};
		editorContainer: { render(width: number): string[] };
		extensionSelector?: unknown;
		loadingAnimation?: unknown;
		createWorkingLoader(): unknown;
		handleImportCommand(text: string): Promise<void>;
	},
	runtime: { session: { messages: unknown[]; sessionFile?: string } },
	terminal: CaptureTerminal,
	request: Request,
): Promise<never> {
	if (!request.importPath) throw new Error("import fixture requires a path");
	const path = request.importPath;
	const dialogs: number[][][] = [];
	let rejected: { name: string; message: string } | null = null;
	mode.statusContainer.addChild(new Text("pre-import status", 0, 0));
	if (request.importFailure === "factory") {
		mode.loadingAnimation = mode.createWorkingLoader();
		mode.statusContainer.addChild(mode.loadingAnimation);
	}
	process.once("exit", (exitCode) => {
		const output = {
			exitCode,
			dialogs,
			rejected,
			chat: mode.chatContainer.render(120).map(units),
			status: mode.statusContainer.render(120).map(units),
			...(request.importFailure === "success" ||
			request.importFailure === "cwd-success"
				? captureImportedSession(runtime.session)
				: {}),
		};
		writeSync(1, `${JSON.stringify({ ok: true, output })}\n`);
	});
	const command =
		request.importFailure === "usage"
			? '/import ""'
			: `/import '${path}' ignored`;
	const operation = mode.handleImportCommand(command);
	let lastFrame = "";
	const driver = setInterval(() => {
		if (!mode.extensionSelector) return;
		const lines = mode.editorContainer.render(120);
		const frame = JSON.stringify(lines);
		if (frame === lastFrame) return;
		lastFrame = frame;
		dialogs.push(lines.map(units));
		const cancel =
			request.importFailure === "cancel" ||
			(request.importFailure === "cwd-cancel" && dialogs.length === 2);
		terminal.input?.(cancel ? "\u001b" : "\r");
	}, 1);
	try {
		await operation;
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		rejected = { name: error.name, message: error.message };
	} finally {
		clearInterval(driver);
	}
	process.exit(0);
}

function captureImportedSession(session: {
	messages: unknown[];
	sessionFile?: string;
}) {
	if (!session.sessionFile)
		throw new Error("successful import has no persisted file");
	return {
		messages: session.messages,
		copiedFile: readFileSync(session.sessionFile, "utf8"),
	};
}

async function captureChanges(
	mode: {
		ui: { renderPendingState(): void };
		editor: { render(width: number): string[] };
	},
	terminal: CaptureTerminal,
	request: Request,
) {
	mode.ui.renderPendingState();
	terminal.writes = [];
	themes.setThemeInstance(
		new themes.Theme(
			Object.fromEntries(request.colors.map((name) => [name, 0])),
			{},
			"256color",
		),
	);
	mode.ui.renderPendingState();
	const themeRepaint = terminal.writes.length > 0;
	const editor = mode.editor.render(120).map(units);
	terminal.writes = [];
	writeFileSync(
		join(request.cwd, ".git", "HEAD"),
		"ref: refs/heads/after-startup\n",
	);
	const deadline = Date.now() + 2000;
	while (
		!terminal.writes.some((text) => text.includes("after-startup")) &&
		Date.now() < deadline
	) {
		await new Promise((resolve) => setTimeout(resolve, 10));
	}
	return {
		themeRepaint,
		editor,
		branchRepaint: terminal.writes.some((text) =>
			text.includes("after-startup"),
		),
	};
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		console.log(
			JSON.stringify({
				ok: true,
				output: await captureStartup(JSON.parse(line)),
			}),
		);
	} catch (error) {
		console.log(
			JSON.stringify({
				ok: false,
				error: String(error),
				stack: error instanceof Error ? error.stack : undefined,
			}),
		);
	}
}
