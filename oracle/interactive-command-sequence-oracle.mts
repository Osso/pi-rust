import { createInterface } from "node:readline";
import { stripVTControlCharacters } from "node:util";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { AgentSession } = await import(pathToFileURL(`${root}/core/agent-session.ts`).href);
const { Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as { inputs: string[] };
		const accepted: string[] = [];
		themes.setThemeInstance(new themes.Theme({ error: 0 }, {}, "256color"));
		const extensionRunner = { getPromptCommand: () => undefined };
		const session: Record<string, unknown> = {
			isStreaming: false,
			isCompacting: false,
			_extensionRunner: extensionRunner,
			extensionRunner,
			promptTemplates: [],
			resourceLoader: { getSkills: () => ({ skills: [] }) },
			_withTurnStartLock: async (action: (release: () => void) => Promise<boolean>) => action(() => {}),
			_promptTurn: async (text: string) => { accepted.push(text); },
			_drainRuntimeCoordinationMessages: async () => {},
		};
		for (const name of ["prompt", "_tryExecuteExtensionCommand", "_promptWithWorkingDirectoryRecovery", "_isKnownSlashCommand", "_slashCommandName"]) {
			session[name] = AgentSession.prototype[name].bind(session);
		}
		const defaultEditor: { onSubmit?: (text: string) => Promise<void> } = {};
		let pending = Promise.resolve();
		const receiver = {
			defaultEditor,
			editor: { setText: () => {} },
			session,
			chatContainer: new Container(),
			isViewingAgentSession: () => false,
			addMessageToChat: () => {},
			ui: { requestRender: () => {} },
			clipboardTempFiles: { cleanupReferencedIn: () => {} },
			closeResponseCompleteNotification: () => {},
			addSubmittedTextToHistory: () => {},
			flushPendingBashComponents: () => {},
			showError: (message: string) => InteractiveMode.prototype.showError.call(receiver, message),
			onInputCallback: (text: string) => {
				pending = InteractiveMode.prototype.submitMainLoopInput.call(receiver, text);
			},
		};
		InteractiveMode.prototype.setupEditorSubmitHandler.call(receiver);
		if (!defaultEditor.onSubmit) throw new Error("submit handler not installed");
		const frames = [];
		for (const text of request.inputs) {
			await defaultEditor.onSubmit(text);
			await pending;
			frames.push({ accepted: [...accepted], errors: receiver.chatContainer.render(120).map((line: string) => stripVTControlCharacters(line).trim()).filter(Boolean) });
		}
		console.log(JSON.stringify({ ok: true, frames }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
