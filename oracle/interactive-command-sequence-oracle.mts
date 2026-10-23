import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { AgentSession } = await import(pathToFileURL(`${root}/core/agent-session.ts`).href);

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as { inputs: string[] };
		const accepted: string[] = [];
		const errors: string[] = [];
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
			chatContainer: { children: [] },
			isViewingAgentSession: () => false,
			addMessageToChat: () => {},
			ui: { requestRender: () => {} },
			clipboardTempFiles: { cleanupReferencedIn: () => {} },
			closeResponseCompleteNotification: () => {},
			addSubmittedTextToHistory: () => {},
			flushPendingBashComponents: () => {},
			showError: (message: string) => { errors.push(message); },
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
			frames.push({ accepted: [...accepted], errors: [...errors] });
		}
		console.log(JSON.stringify({ ok: true, frames }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
