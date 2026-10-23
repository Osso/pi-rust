import { writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { stripVTControlCharacters } from "node:util";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(
	pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${root}/core/session-manager.ts`).href
);
const { assertSessionCwdExists } = await import(
	pathToFileURL(`${root}/core/session-cwd.ts`).href
);
const { Container } = await import(
	pathToFileURL(`${root}/../../tui/src/index.ts`).href
);
const themes = await import(
	pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href
);
const units = (text: string) =>
	Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const palette = Object.fromEntries(
			request.colors.map((name: string) => [name, 0]),
		);
		themes.setThemeInstance(new themes.Theme(palette, palette, "256color"));
		const confirmations: unknown[] = [];
		const receiver = {
			statusContainer: new Container(),
			chatContainer: new Container(),
			ui: { requestRender() {} },
			createProjectTrustContext() {},
			runtimeHost: {
				async switchSession(path: string) {
					const manager = SessionManager.open(path);
					try {
						assertSessionCwdExists(
							manager,
							String.fromCharCode(...request.fallback),
						);
					} catch (error) {
						writeFileSync(path, request.changedHeader);
						throw error;
					}
					throw new Error("fixture expected a missing cwd");
				},
			},
			async showExtensionConfirm(title: string, message: string) {
				confirmations.push({ title: units(title), message: units(message) });
				return false;
			},
			promptForMissingSessionCwd(error: unknown) {
				return InteractiveMode.prototype.promptForMissingSessionCwd.call(
					receiver,
					error,
				);
			},
			showStatus(message: string) {
				InteractiveMode.prototype.showStatus.call(receiver, message);
			},
		};
		const result = await InteractiveMode.prototype.handleResumeSession.call(
			receiver,
			request.file,
		);
		const frames = receiver.chatContainer
			.render(200)
			.map((line: string) => stripVTControlCharacters(line).trim())
			.filter(Boolean)
			.map(units);
		console.log(
			JSON.stringify({
				ok: true,
				output: { confirmations, frames, cancelled: result.cancelled },
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
