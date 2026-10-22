import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const [source, requestJson, trace] = process.argv.slice(2);
	const request: {
		command: string;
		stopError?: boolean;
		resumeError?: boolean;
		tmpdir?: string;
	} = JSON.parse(requestJson!);
	const { ExtensionEditorComponent } = await import(
		pathToFileURL(`${source}/modes/interactive/components/extension-editor.ts`)
			.href
	);
	const { KeybindingsManager } = await import(
		pathToFileURL(`${source}/core/keybindings.ts`).href
	);
	const themes = await import(
		pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href
	);
	themes.setThemeInstance(
		new themes.Theme(
			{ accent: 0, text: 0, muted: 0, dim: 0, border: 0 },
			{},
			"256color",
		),
	);
	const events: string[] = [];
	const record = (event: string): void => {
		events.push(event);
		writeFileSync(trace!, JSON.stringify(events));
	};
	const host = {
		terminal: { rows: 24 },
		requestRender(force?: boolean) {
			if (force) record("render");
		},
		createFixedCell() {
			return {
				place() {},
				clear() {},
				update() {
					return false;
				},
			};
		},
		stop() {
			record("stop");
			if (request.stopError) throw new Error("stop fixture");
		},
		start() {
			record("start");
			if (request.resumeError)
				throw new Error("Failed to enable terminal raw mode: resume fixture");
		},
	};
	const editor = new ExtensionEditorComponent(
		host,
		new KeybindingsManager(),
		"Compose",
		"original",
		() => {},
		() => {},
		undefined,
		request.command,
	);
	if (request.tmpdir !== undefined) process.env.TMPDIR = request.tmpdir;
	editor.handleInput("\x07");
	record("input-returned");
	setTimeout(() => {
		throw new Error(
			"uncaught external editor error did not reach process boundary",
		);
	}, 4000).unref();
}
void main();
