import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const load = (path: string) => import(pathToFileURL(`${source}/${path}`).href);
const { InteractiveMode } = await load("modes/interactive/interactive-mode.ts");
const { SettingsManager } = await load("core/settings-manager.ts");
const { KeybindingsManager } = await load("core/keybindings.ts");
const { Container, Text, setKeybindings } = await load(
	"../../tui/src/index.ts",
);
const { Theme, setThemeInstance } = await load(
	"modes/interactive/theme/theme.ts",
);
const config = await load("config.ts");
const units = (text: string) =>
	Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
interface Request {
	case: string;
	root: string;
	colors: string[];
	settings: Record<string, unknown>;
	markdown: string | null;
	resumed: boolean;
	existing: boolean;
	offline: string | null;
	telemetry: string;
	shortcuts: { key: string; description?: string; extensionPath: string }[];
}
async function capture(request: Request) {
	if (request.offline === null) delete process.env.PI_OFFLINE;
	else process.env.PI_OFFLINE = request.offline;
	process.env.PI_TELEMETRY = request.telemetry;
	const requests: unknown[] = [];
	globalThis.fetch = async (input, init) => {
		requests.push({
			url: String(input),
			method: init?.method ?? "GET",
			headers: Object.entries(init?.headers ?? {}),
		});
		if (request.case === "telemetry-failure")
			throw new Error("fixture network rejection");
		return {} as Response;
	};
	setThemeInstance(
		new Theme(
			Object.fromEntries(request.colors.map((name) => [name, 0])),
			{},
			"256color",
		),
	);
	const settings = SettingsManager.inMemory(request.settings);
	const bindings = KeybindingsManager.create(request.root);
	setKeybindings(bindings);
	const session = {
		state: { messages: request.resumed ? [{}] : [] },
		settingsManager: settings,
		extensionRunner: {
			getShortcuts: () =>
				new Map(request.shortcuts.map((shortcut) => [shortcut.key, shortcut])),
		},
	};
	const mode = Object.assign(Object.create(InteractiveMode.prototype), {
		runtimeHost: { session },
		keybindings: bindings,
		version: config.VERSION,
		changelogMarkdown: request.markdown ?? undefined,
		startupNoticesShown: false,
		chatContainer: new Container(),
		ui: { requestRender() {} },
	});
	if (request.existing)
		mode.chatContainer.addChild(new Text("Existing chat", 0, 0));
	let markdown: string | null = null;
	const frames = [];
	if (request.case.startsWith("hotkeys")) mode.handleHotkeysCommand();
	else if (request.case.startsWith("changelog")) mode.handleChangelogCommand();
	else if (request.case.startsWith("notice")) {
		mode.showStartupNoticesIfNeeded();
		frames.push(
			[25, 120].map((width) => mode.chatContainer.render(width).map(units)),
		);
		mode.showStartupNoticesIfNeeded();
	} else if (request.case.startsWith("telemetry"))
		mode.reportInstallTelemetry("9.8.7+build");
	else markdown = mode.getChangelogForDisplay() ?? null;
	await Promise.resolve();
	await Promise.resolve();
	frames.push(
		[25, 120].map((width) => mode.chatContainer.render(width).map(units)),
	);
	return {
		frames,
		markdown,
		seen: settings.getLastChangelogVersion() ?? null,
		requests,
		shown: mode.startupNoticesShown,
	};
}
for await (const line of createInterface({ input: process.stdin })) {
	try {
		console.log(
			JSON.stringify({
				ok: true,
				output: await capture(JSON.parse(line) as Request),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
