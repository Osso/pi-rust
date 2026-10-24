import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(
	pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href
);
const { KeybindingsManager } = await import(
	pathToFileURL(`${root}/core/keybindings.ts`).href
);
const { Container, setKeybindings } = await import(
	pathToFileURL(`${root}/../../tui/src/index.ts`).href
);
const themes = await import(
	pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href
);
interface Shortcut {
	key: string;
	description?: string | null;
	extensionPath: string;
}
interface Request {
	width: number;
	colors: string[];
	bindings?: Record<string, string[]>;
	shortcuts: Shortcut[];
}
const units = (text: string) =>
	Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		themes.setThemeInstance(
			new themes.Theme(
				Object.fromEntries(request.colors.map((name) => [name, 0])),
				{},
				"256color",
			),
		);
		const keybindings = new KeybindingsManager(request.bindings ?? {});
		setKeybindings(keybindings);
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			chatContainer: new Container(),
			keybindings,
			ui: { requestRender: () => {} },
			runtimeHost: {
				session: {
					extensionRunner: {
						getShortcuts: () =>
							new Map(
								request.shortcuts.map((shortcut) => [shortcut.key, shortcut]),
							),
					},
				},
			},
			getMarkdownThemeWithSettings: () => themes.getMarkdownTheme(),
		});
		InteractiveMode.prototype.handleHotkeysCommand.call(receiver);
		console.log(
			JSON.stringify({
				ok: true,
				output: receiver.chatContainer.render(request.width).map(units),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
