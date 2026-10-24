import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		themes.setThemeInstance(new themes.Theme(Object.fromEntries(request.colors.map((name: string) => [name, 0])), {}, "256color"));
		const extensions = request.commands.map((command: { sourceInfo: { path: string } }) => ({ path: command.sourceInfo.path, sourceInfo: command.sourceInfo }));
		const session = {
			settingsManager: { getQuietStartup: () => true },
			resourceLoader: {
				getSkills: () => ({ skills: [], diagnostics: [] }),
				getPrompts: () => ({ prompts: [], diagnostics: [] }),
				getThemes: () => ({ themes: [], diagnostics: [] }),
				getExtensions: () => ({ extensions, errors: [] }),
			},
			getActiveToolNames: () => [],
			extensionRunner: {
				getRegisteredCommands: () => request.commands,
				getCommandDiagnostics: () => [],
				getShortcutDiagnostics: () => [],
			},
		};
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			runtimeHost: { session }, options: { verbose: false }, loadedResourcesContainer: new Container(), ui: { requestRender() {} },
		});
		receiver.showLoadedResources({ showDiagnosticsWhenQuiet: true });
		console.log(JSON.stringify({ ok: true, output: receiver.loadedResourcesContainer.render(request.width).map(units) }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
