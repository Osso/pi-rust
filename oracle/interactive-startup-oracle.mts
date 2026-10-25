import { mkdirSync, writeFileSync, writeSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import type { Model } from "@pi-oracle-source/ai/src/types.ts";
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
	importFailure?: "missing" | "factory";
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
	const sessionManager = request.importFailure
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
	const runtime = new AgentSessionRuntime(
		result.session,
		{
			cwd,
			agentDir,
			authStorage,
			modelRegistry,
			resourceLoader: loader,
			settingsManager,
			diagnostics: [],
		},
		async () => {
			throw new Error("startup fixture never replaces session");
		},
	);
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
		handleImportCommand(text: string): Promise<void>;
	},
	runtime: { createRuntime: () => Promise<never> },
	terminal: CaptureTerminal,
	request: Request,
): Promise<never> {
	if (!request.importPath) throw new Error("import fixture requires a path");
	process.once("exit", (exitCode) => {
		const output = {
			exitCode,
			chat: mode.chatContainer.render(120).map(units),
		};
		writeSync(1, `${JSON.stringify({ ok: true, output })}\n`);
	});
	if (request.importFailure === "factory") {
		runtime.createRuntime = async () => {
			throw new Error("File not found: imported dependency");
		};
	}
	const operation = mode.handleImportCommand(`/import ${request.importPath}`);
	terminal.input?.("\r");
	await operation;
	process.exit(0);
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
