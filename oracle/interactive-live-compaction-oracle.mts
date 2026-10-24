import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(
	pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${root}/core/session-manager.ts`).href
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
const units = (text: string) =>
	Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const colors = Object.fromEntries(
			request.colors.map((name: string) => [name, 0]),
		);
		themes.setThemeInstance(new themes.Theme(colors, colors, "256color"));
		setKeybindings(new KeybindingsManager({}));
		const manager = SessionManager.inMemory(request.cwd);
		const first = manager.appendMessage({
			role: "user",
			content: "kept user",
			timestamp: 0,
		});
		let entryId: string | undefined;
		if (request.persisted)
			entryId = manager.appendCompaction(
				"persisted summary",
				first,
				999,
				undefined,
				false,
				31,
			);
		const receiver = Object.create(InteractiveMode.prototype);
		receiver.runtimeHost = {
			session: {
				sessionManager: manager,
				promptTemplates: [],
				settingsManager: {
					isProjectTrusted: () => true,
					getHideCoordinationMessages: () => false,
					getCodeBlockIndent: () => "  ",
				},
				updateCompactionSummary: (id: string, summary: string) =>
					manager.updateCompactionSummary(id, summary),
			},
		};
		receiver.chatContainer = new Container();
		receiver.ui = { requestRender() {} };
		receiver.footer = { invalidate() {} };
		receiver.toolOutputExpanded = request.expanded;
		receiver.compactionQueuedMessages = [];
		receiver.pendingTools = new Map();
		receiver.completedToolTimings = new Map();
		receiver.executingToolStartedAt = new Map();
		receiver.updateEditorBorderColor = () => {};
		receiver.addRenderedMessageToEditorHistory = () => {};
		const history: string[] = [];
		let editor = "";
		receiver.editor = {
			getText: () => editor,
			setText: (text: string) => {
				editor = text;
			},
			addToHistory: (text: string) => history.push(text),
		};
		const frames: number[][][] = [];
		receiver.rebuildChatAfterCompaction(
			request.result,
			"2026-10-24T12:34:56.789Z",
		);
		frames.push(receiver.chatContainer.render(request.width).map(units));
		for (const summary of request.edits) {
			if (!entryId) throw new Error("fixture edit needs persisted compaction");
			receiver.startCompactionSummaryEdit(manager.getEntry(entryId));
			receiver.submitPendingCompactionSummaryEdit(summary);
			frames.push(receiver.chatContainer.render(request.width).map(units));
		}
		const snapshot = {
			ok: true,
			frames,
			history: history.map(units),
			editor: units(editor),
			timestamp: receiver.liveCompactionDisplay?.timestamp ?? null,
		};
		let actions:
			| { navigateTree: (id: string) => Promise<{ cancelled: boolean }> }
			| undefined;
		receiver.session.bindExtensions = async (bindings: {
			commandContextActions: typeof actions;
		}) => {
			actions = bindings.commandContextActions;
		};
		receiver.session.resourceLoader = { getThemes: () => ({ themes: [] }) };
		receiver.session.extensionRunner = {};
		receiver.session.navigateTree = async () => {
			if (request.cancelNavigation) return { cancelled: true };
			if (request.persisted) manager.resetLeaf();
			return {
				cancelled: false,
				editorText: request.persisted ? "kept user" : undefined,
			};
		};
		receiver.options = {};
		receiver.createExtensionUIContext = () => ({});
		receiver.setupAutocompleteProvider = () => {};
		receiver.setupExtensionShortcuts = () => {};
		receiver.showLoadedResources = () => {};
		receiver.showStartupNoticesIfNeeded = () => {};
		await receiver.bindCurrentSessionExtensions();
		if (!actions) throw new Error("source did not bind navigation");
		if (request.editorOccupied) editor = "occupied editor";
		const navigationResult = await actions.navigateTree(first);
		console.log(
			JSON.stringify({
				...snapshot,
				navigation: {
					cancelled: navigationResult.cancelled,
					frames: receiver.chatContainer.render(request.width).map(units),
					editor: units(editor),
					liveCleared: receiver.liveCompactionDisplay === undefined,
					pendingCleared: receiver.pendingCompactionSummaryEditId === undefined,
				},
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
