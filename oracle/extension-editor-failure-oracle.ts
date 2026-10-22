import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const [source, command, trace] = process.argv.slice(2);
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
		},
		start() {
			record("start");
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
		command,
	);
	editor.handleInput("\x07");
	record("input-returned");
	setTimeout(() => {
		throw new Error(
			"uncaught external editor error did not reach process boundary",
		);
	}, 4000).unref();
}
void main();
