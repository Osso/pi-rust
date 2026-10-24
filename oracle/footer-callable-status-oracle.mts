import fs from "node:fs";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
const requestPath = process.argv[3];
if (!root || !requestPath) throw new Error("source root and request required");
const request = JSON.parse(fs.readFileSync(requestPath, "utf8"));
const { FooterDataProvider } = await import(pathToFileURL(`${root}/core/footer-data-provider.ts`).href);
const { FooterComponent } = await import(pathToFileURL(`${root}/modes/interactive/components/footer.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
themes.setThemeInstance(new themes.Theme({ dim: 0, error: 0, warning: 0 }, {}, "256color"));
const provider = new FooterDataProvider(request.cwd);
const session = {
	state: { model: request.model, thinkingLevel: "high" },
	sessionManager: { getEntries: () => [], getSessionName: () => undefined, getCwd: () => request.cwd },
	getContextUsage: () => undefined,
	modelRegistry: { isUsingOAuth: () => false },
};
const footer = new FooterComponent(session, provider);
const failure = Object.assign(new TypeError("status-\ud800"), { code: "ESTATUS" });
const errorValue = (error: Error) => Object.assign({ name: error.name, message: error.message }, error);
const trace: unknown[] = [];
let status = "first-\ud800";
let omitted = false;
provider.setExtensionStatus("dynamic", (override: { cwd: string; thinkingLevel: string } | undefined) => {
	trace.push(override === undefined ? null : { cwd: override.cwd, thinkingLevel: override.thinkingLevel });
	return omitted ? undefined : status;
});
provider.setExtensionStatus("static", "static line");
const snapshots: unknown[] = [{ trace: [...trace] }];
const read = () => snapshots.push({ statuses: [...provider.getExtensionStatuses()], trace: [...trace] });
const render = () => snapshots.push({ lines: footer.render(200).slice(2), trace: [...trace] });

if (request.operation === "sequence") {
	render();
	status = "second-\udc00";
	read();
	provider.setSessionOverride({ cwd: "view-\ud800", sessionManager: null, model: null, thinkingLevel: "low", contextUsage: undefined });
	read();
	render();
	omitted = true;
	read();
	omitted = false;
	provider.clearSessionOverride();
	read();
	provider.setExtensionStatus("dynamic", "replaced");
	read();
	provider.clearExtensionStatuses();
	read();
	provider.dispose();
	console.log(JSON.stringify(snapshots));
} else {
	provider.clearExtensionStatuses();
	provider.setExtensionStatus("throw", () => {
		trace.push(null);
		throw failure;
	});
	if (request.operation === "uncaught") {
		// tui.ts:962-1013 schedules the real render; :1897-1902 has finally, no catch.
		const { TUI } = await import(pathToFileURL(`${root}/../../tui/src/tui.ts`).href);
		const terminal = {
			columns: 200, rows: 24,
			start() {}, stop() {}, write() {}, hideCursor() {}, showCursor() {},
			moveBy() {}, clearLine() {}, clearFromCursor() {}, clearScreen() {},
			setTitle() {}, setProgress() {}, drainInput() {},
		};
		const tui = new TUI(terminal);
		process.once("uncaughtException", (error) => {
			console.log(JSON.stringify({ uncaught: true, sameError: error === failure, error: errorValue(error), trace }));
			process.exit(17);
		});
		tui.addChild(footer);
		tui.start();
		setTimeout(() => process.exit(99), 2000);
	} else {
		try {
			if (request.operation === "checked") footer.render(200);
			else provider.getExtensionStatuses();
			throw new Error("callback did not throw");
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			console.log(JSON.stringify({ sameError: error === failure, error: errorValue(error), trace }));
		}
		provider.dispose();
	}
}
