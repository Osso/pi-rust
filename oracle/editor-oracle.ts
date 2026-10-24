import { pathToFileURL } from "node:url";
import * as path from "node:path";
import { selectApprovedModule } from "./approved-delta.mjs";

type RequestId = string | number | null;

type Request = {
	id: RequestId;
	ops: Operation[];
	theme?: ThemeRequest;
	options?: OptionsRequest;
};

type ThemeRequest = {
	promptPrefix?: boolean;
	borderColor?: boolean;
};

type OptionsRequest = {
	paddingX?: number;
	autocompleteMaxVisible?: number;
};

type Operation =
	| { op: "input"; data: string }
	| { op: "setText"; text: string }
	| { op: "insertText"; text: string }
	| { op: "addHistory"; text: string }
	| { op: "setHistory"; entries: string[] }
	| { op: "render"; width: number }
	| { op: "settle" }
	| { op: "focus"; focused: boolean }
	| { op: "setProvider"; provider: ProviderRequest }
	| { op: "setPaddingX"; padding: number }
	| { op: "setAutocompleteMaxVisible"; maxVisible: number }
	| { op: "setWorking"; working: boolean }
	| { op: "setWorkingIndicator"; frames: string[]; intervalMs?: number }
	| { op: "setScreenOrigin"; row: number; col: number }
	| { op: "clearScreenOrigin" };

type ProviderRequest = {
	items: Array<{ value: string; label: string; description?: string }>;
	mode: "prefix" | "all";
	requireForce?: boolean;
	shouldTrigger?: boolean | null;
	triggerCharacters?: string[];
	prefix?: string | null;
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function loadApi(
	sourceDirectory: string,
): Promise<{ Editor: any; TUI: any; VirtualTerminal: any }> {
	// This oracle intentionally and exclusively proves the approved editor delta.
	process.env.PI_APPROVED_DELTA = "1";
	const editorPath = selectApprovedModule(
		path.join(sourceDirectory, "components/editor.ts"),
		"packages/tui/src/components/editor.ts",
	);
	const editor = await import(pathToFileURL(editorPath).href);
	const mainScreen = await import(
		pathToFileURL(path.join(sourceDirectory, "tui.ts")).href
	);
	const virtualTerminal = await import(
		pathToFileURL(path.join(sourceDirectory, "../test/virtual-terminal.ts"))
			.href
	);
	return {
		Editor: editor.Editor,
		TUI: mainScreen.TUI,
		VirtualTerminal: virtualTerminal.VirtualTerminal,
	};
}

function applyCompletion(
	lines: string[],
	cursorLine: number,
	cursorCol: number,
	item: { value: string },
	prefix: string,
): { lines: string[]; cursorLine: number; cursorCol: number } {
	const line = lines[cursorLine] ?? "";
	const before = line.slice(0, cursorCol - prefix.length);
	const after = line.slice(cursorCol);
	const newLines = [...lines];
	newLines[cursorLine] = before + item.value + after;
	return {
		lines: newLines,
		cursorLine,
		cursorCol: cursorCol - prefix.length + item.value.length,
	};
}

function makeProvider(config: ProviderRequest): unknown {
	const items = config.items.map((item) => ({ ...item }));
	return {
		triggerCharacters: config.triggerCharacters,
		shouldTriggerFileCompletion:
			config.shouldTrigger === null || config.shouldTrigger === undefined
				? undefined
				: () => config.shouldTrigger,
		getSuggestions: async (
			lines: string[],
			cursorLine: number,
			cursorCol: number,
			options: { force?: boolean },
		) => {
			if (config.requireForce && !options.force) return null;
			const line = lines[cursorLine] ?? "";
			const prefix = line.slice(0, cursorCol);
			let filtered = items;
			if (config.mode === "prefix") {
				const matchPrefix = config.prefix ?? prefix;
				filtered = items.filter((item) => item.value.startsWith(matchPrefix));
			}
			if (filtered.length === 0) return null;
			return { items: filtered, prefix: config.prefix ?? prefix };
		},
		applyCompletion,
	};
}

async function flushAutocomplete(): Promise<void> {
	await Promise.resolve();
	await new Promise((resolve) => setTimeout(resolve, 50));
	await Promise.resolve();
}

async function execute(
	request: Request,
	sourceDirectory: string,
): Promise<unknown> {
	const { Editor, TUI, VirtualTerminal } = await loadApi(sourceDirectory);
	const terminal = new VirtualTerminal(80, 24);
	const tui = new TUI(terminal);
	const theme: any = {
		borderColor: request.theme?.borderColor
			? (text: string) => `\x1b[35m${text}\x1b[39m`
			: (text: string) => text,
		selectList: {
			selectedPrefix: (text: string) => text,
			selectedText: (text: string) => text,
			description: (text: string) => text,
			scrollInfo: (text: string) => text,
			noMatch: (text: string) => text,
		},
	};
	if (request.theme?.promptPrefix) theme.promptPrefix = (text: string) => text;
	const editor = new Editor(tui, theme, request.options ?? {});
	const changes: string[] = [];
	const submissions: string[] = [];
	editor.onChange = (text: string) => changes.push(text);
	editor.onSubmit = (text: string) => submissions.push(text);
	const renders: string[][] = [];

	for (const operation of request.ops) {
		switch (operation.op) {
			case "input":
				editor.handleInput(operation.data);
				break;
			case "setText":
				editor.setText(operation.text);
				break;
			case "insertText":
				editor.insertTextAtCursor(operation.text);
				break;
			case "addHistory":
				editor.addToHistory(operation.text);
				break;
			case "setHistory":
				editor.setHistory(operation.entries);
				break;
			case "render":
				renders.push(editor.render(operation.width));
				break;
			case "settle":
				await flushAutocomplete();
				break;
			case "focus":
				editor.focused = operation.focused;
				break;
			case "setProvider":
				editor.setAutocompleteProvider(makeProvider(operation.provider));
				break;
			case "setPaddingX":
				editor.setPaddingX(operation.padding);
				break;
			case "setAutocompleteMaxVisible":
				editor.setAutocompleteMaxVisible(operation.maxVisible);
				break;
			case "setWorking":
				editor.setWorking(operation.working);
				break;
			case "setWorkingIndicator":
				editor.setWorkingIndicator({
					frames: operation.frames,
					intervalMs: operation.intervalMs,
				});
				break;
			case "setScreenOrigin":
				editor.setScreenOrigin(operation.row, operation.col);
				break;
			case "clearScreenOrigin":
				editor.clearScreenOrigin();
				break;
		}
	}

	const cursor = editor.getCursor();
	return {
		renders,
		text: editor.getText(),
		expanded: editor.getExpandedText(),
		cursor: { line: cursor.line, col: cursor.col },
		changes,
		submissions,
		autocomplete: editor.isShowingAutocomplete(),
	};
}

const sourceDirectory = process.argv[2];
if (!sourceDirectory)
	throw new Error("editor oracle requires the source directory argument");

process.stdin.setEncoding("utf8");
let buffer = "";
process.stdin.on("data", (chunk: string) => {
	buffer += chunk;
	let index = buffer.indexOf("\n");
	while (index !== -1) {
		const line = buffer.slice(0, index);
		buffer = buffer.slice(index + 1);
		index = buffer.indexOf("\n");
		if (!line.trim()) continue;
		const request = JSON.parse(line) as Request;
		execute(request, sourceDirectory)
			.then((output) => {
				const response: Response = { id: request.id, ok: true, output };
				process.stdout.write(`${JSON.stringify(response)}\n`);
			})
			.catch((error: unknown) => {
				const response: Response = {
					id: request.id,
					ok: false,
					error: normalizeError(error),
				};
				process.stdout.write(`${JSON.stringify(response)}\n`);
			});
	}
});
