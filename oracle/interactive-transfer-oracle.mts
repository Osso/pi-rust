import { existsSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import type { Terminal } from "@pi-oracle-source/tui/src/terminal.ts";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const load = (path: string) => import(pathToFileURL(`${source}/${path}`).href);
const { AgentSession } = await load("core/agent-session.ts");
const { SessionManager } = await load("core/session-manager.ts");
const { SettingsManager } = await load("core/settings-manager.ts");
const { InteractiveMode } = await load("modes/interactive/interactive-mode.ts");
const { Agent } = await load("../../agent-core/src/agent.ts");
const { Container, Input, TUI } = await load("../../tui/src/index.ts");
const { initTheme, loadThemeFromPath, setRegisteredThemes } = await load(
	"modes/interactive/theme/theme.ts",
);
const units = (text: string) =>
	Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
class CaptureTerminal implements Terminal {
	columns = 120;
	rows = 40;
	kittyProtocolActive = false;
	input?: (text: string) => void;
	start(onInput: (text: string) => void) {
		this.input = onInput;
	}
	stop() {}
	async drainInput() {}
	write() {}
	moveBy() {}
	hideCursor() {}
	showCursor() {}
	clearLine() {}
	clearFromCursor() {}
	clearScreen() {}
	setTitle() {}
}
interface Request {
	cwd: string;
	case: string;
	sessionPath: string;
	model: unknown;
	exportPath: string;
	clock?: string | null;
	colors: string[];
}
async function capture(request: Request) {
	const cwd = process.cwd();
	const path = process.env.PATH;
	const tmpdir = process.env.TMPDIR;
	const wayland = process.env.WAYLAND_DISPLAY;
	const isolated = [
		"SSH_CONNECTION",
		"SSH_CLIENT",
		"MOSH_CONNECTION",
		"TERMUX_VERSION",
		"XDG_SESSION_TYPE",
	];
	const previous = isolated.map((name) => process.env[name]);
	const date = globalThis.Date;
	for (const name of ["ready", "terminated", "clipboard"])
		rmSync(join(request.cwd, name), { force: true });
	setRegisteredThemes([
		loadThemeFromPath(join(request.cwd, "transfer-theme.json"), "256color"),
	]);
	initTheme("transfer-fixture", false);
	const manager = SessionManager.open(request.sessionPath);
	const session = Object.create(AgentSession.prototype);
	session.sessionManager = manager;
	session.settingsManager = SettingsManager.inMemory({});
	session.agent = new Agent({
		initialState: {
			model: request.model,
			messages: manager.buildSessionContext().messages,
		},
	});
	const terminal = new CaptureTerminal();
	const ui = new TUI(terminal);
	const editor = new Input();
	const editorContainer = new Container();
	editorContainer.addChild(editor);
	const mode = Object.assign(Object.create(InteractiveMode.prototype), {
		runtimeHost: { session },
		ui,
		editor,
		editorContainer,
		chatContainer: new Container(),
	});
	ui.addChild(editorContainer);
	ui.setFocus(editor);
	ui.start();
	let loaderFrame: number[][] = [];
	try {
		process.chdir(request.cwd);
		process.env.PATH = join(request.cwd, "bin");
		process.env.TMPDIR = join(request.cwd, "tmp");
		process.env.WAYLAND_DISPLAY = "fixture-wayland";
		for (const name of isolated) delete process.env[name];
		process.env.XDG_SESSION_TYPE = "wayland";
		if (request.clock) {
			globalThis.Date = new Proxy(date, {
				construct(target, args) {
					return Reflect.construct(
						target,
						args.length ? args : [request.clock],
					);
				},
			});
		}
		if (request.case.startsWith("export")) {
			await mode.handleExportCommand(`/export '${request.exportPath}' ignored`);
		} else if (request.case.startsWith("copy")) {
			await mode.handleCopyCommand();
		} else {
			const operation = mode.handleShareCommand();
			if (request.case === "share-cancel") {
				const ready = join(request.cwd, "ready");
				for (let attempt = 0; !existsSync(ready); attempt++) {
					if (attempt === 2000) throw new Error("fake gh never became ready");
					await new Promise((resolve) => setTimeout(resolve, 1));
				}
				loaderFrame = editorContainer.render(120).map(units);
				terminal.input?.("\x1b");
			}
			await operation;
		}
		const artifact =
			request.case === "export-jsonl"
				? readFileSync(request.exportPath, "utf8")
				: null;
		const html =
			request.case.startsWith("export") &&
			readFileSync(request.exportPath, "utf8")
				.trimStart()
				.toLowerCase()
				.startsWith("<!doctype html>");
		return {
			chat: mode.chatContainer.render(120).map(units),
			loaderFrame,
			editorFocused: ui.getFocusedComponent() === editor,
			temporaryExists: existsSync(join(request.cwd, "tmp/session.html")),
			terminated: existsSync(join(request.cwd, "terminated")),
			clipboard: existsSync(join(request.cwd, "clipboard"))
				? [...readFileSync(join(request.cwd, "clipboard"))]
				: [],
			artifact,
			html,
		};
	} finally {
		ui.stop();
		process.chdir(cwd);
		if (path === undefined) delete process.env.PATH;
		else process.env.PATH = path;
		if (tmpdir === undefined) delete process.env.TMPDIR;
		else process.env.TMPDIR = tmpdir;
		if (wayland === undefined) delete process.env.WAYLAND_DISPLAY;
		else process.env.WAYLAND_DISPLAY = wayland;
		isolated.forEach((name, i) => {
			if (previous[i] === undefined) delete process.env[name];
			else process.env[name] = previous[i];
		});
		globalThis.Date = date;
	}
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
