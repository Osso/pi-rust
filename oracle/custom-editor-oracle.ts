import { createInterface } from "node:readline";
import { mkdirSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

type Action = { name: string; handled: boolean; text?: number[] };
type Request = {
	bindings: Record<string, string[]>;
	actions: Action[];
	text: number[];
	input: number[];
	history: string[];
	shortcut?: { handled: boolean; text?: number[] };
	escape?: boolean;
	exit?: boolean;
	paste?: boolean;
};
async function main(): Promise<void> {
	const source = process.argv[2]!;
	const { CustomEditor } = await import(
		pathToFileURL(`${source}/modes/interactive/components/custom-editor.ts`)
			.href
	);
	const { KeybindingsManager } = await import(
		pathToFileURL(`${source}/core/keybindings.ts`).href
	);
	const { createSqliteDatabase } = await import(
		pathToFileURL(`${source}/core/sqlite.ts`).href
	);
	const identity = (text: string): string => text;
	const theme = {
		borderColor: identity,
		selectList: {
			selectedPrefix: identity,
			selectedText: identity,
			description: identity,
			scrollInfo: identity,
			noMatch: identity,
		},
	};
	const host = {
		terminal: { rows: 24 },
		requestRender() {},
		createFixedCell() {
			return {
				place() {},
				clear() {},
				update() {
					return false;
				},
			};
		},
	};
	const units = (text: string): number[] =>
		Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
	const text = (value: number[]): string => String.fromCharCode(...value);

	function history(request: {
		directory: string;
		legacy: (number[] | null)[];
		additions: number[][];
	}): unknown {
		const database = `${request.directory}/control.db`;
		const legacy = `${request.directory}/legacy.json`;
		writeFileSync(
			legacy,
			JSON.stringify(
				request.legacy.map((entry) => (entry === null ? 42 : text(entry))),
			),
		);
		const editor = new CustomEditor(host, theme, new KeybindingsManager(), {
			promptHistoryControlDbPath: database,
			legacyPromptHistoryPath: legacy,
			promptHistoryFilter: (entry: string) => entry !== "drop",
		});
		const snapshots = [editor.getHistory().map(units)];
		for (const addition of request.additions) {
			editor.addToHistory(text(addition));
			snapshots.push(editor.getHistory().map(units));
		}
		editor.addToHistoryWithoutPersistence("memory-only");
		snapshots.push(editor.getHistory().map(units));
		const reopened = new CustomEditor(host, theme, new KeybindingsManager(), {
			promptHistoryControlDbPath: database,
		});
		snapshots.push(reopened.getHistory().map(units));
		const connection = createSqliteDatabase(database);
		const stored = connection
			.prepare(
				"SELECT hex(CAST(content AS BLOB)) AS bytes FROM prompt_history ORDER BY id",
			)
			.all()
			.map((row: { bytes: string }) => row.bytes);
		connection.close();
		return { snapshots, stored };
	}

	function legacyVariant(request: {
		directory: string;
		kind: string;
		bytes: number[] | null;
	}): unknown {
		const legacy = `${request.directory}/legacy.json`;
		if (request.bytes) writeFileSync(legacy, Buffer.from(request.bytes));
		if (request.kind === "directory") mkdirSync(legacy);
		const editor = new CustomEditor(host, theme, new KeybindingsManager(), {
			promptHistoryControlDbPath: `${request.directory}/control.db`,
			legacyPromptHistoryPath: request.kind === "absent" ? undefined : legacy,
		});
		return editor.getHistory().map(units);
	}

	function historyFailure(directory: string): unknown {
		const database = `${directory}/control.db`;
		const editor = new CustomEditor(host, theme, new KeybindingsManager(), {
			promptHistoryControlDbPath: database,
		});
		const connection = createSqliteDatabase(database);
		connection.exec(
			"CREATE TRIGGER deny_history BEFORE INSERT ON prompt_history BEGIN SELECT RAISE(ABORT, 'history-write-failed'); END;",
		);
		const outcomes = ["first", "first", "second"].map((entry) => {
			try {
				editor.addToHistory(entry);
				return { ok: true };
			} catch (error) {
				return { ok: false, message: (error as Error).message };
			}
		});
		const stored = connection
			.prepare("SELECT count(*) AS count FROM prompt_history")
			.get().count;
		connection.close();
		return { outcomes, history: editor.getHistory().map(units), stored };
	}

	async function autocomplete(): Promise<unknown> {
		const { CombinedAutocompleteProvider } = await import(
			pathToFileURL(`${source}/../../tui/src/autocomplete.ts`).href
		);
		const editor = new CustomEditor(host, theme, new KeybindingsManager());
		editor.setAutocompleteProvider(
			new CombinedAutocompleteProvider(
				[{ name: "alpha" }, { name: "beta" }],
				"/tmp",
			),
		);
		editor.setText("/");
		const events: string[] = [];
		editor.onEscape = () => events.push("escape");
		editor.onAction("app.interrupt", () => {
			events.push("registered");
			return false;
		});
		editor.handleInput("\t");
		await new Promise((resolve) => setTimeout(resolve, 0));
		const before = editor.isShowingAutocomplete();
		editor.handleInput("\x1b");
		const after = editor.isShowingAutocomplete();
		editor.handleInput("\x1b");
		return { before, after, events, text: units(editor.getText()) };
	}

	function dispatch(request: Request): unknown {
		const editor = new CustomEditor(
			host,
			theme,
			new KeybindingsManager(request.bindings),
		);
		editor.setText(text(request.text));
		editor.setHistory(request.history);
		const events: unknown[] = [];
		const record = (name: string, next?: number[]): void => {
			events.push([name, units(editor.getText())]);
			if (next !== undefined) editor.setText(text(next));
		};
		for (const action of request.actions) {
			editor.onAction(action.name, () => {
				record(action.name, action.text);
				return action.handled;
			});
		}
		if (request.shortcut) {
			const shortcut = request.shortcut;
			editor.onExtensionShortcut = () => {
				record("shortcut", shortcut.text);
				return shortcut.handled;
			};
		}
		if (request.escape) editor.onEscape = () => record("escape");
		if (request.exit) editor.onCtrlD = () => record("exit");
		if (request.paste) editor.onPasteImage = () => record("paste");
		editor.handleInput(text(request.input));
		return {
			text: units(editor.getText()),
			history: editor.getHistory().map(units),
			events,
		};
	}

	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line);
			const output =
				request.operation === "history"
					? history(request)
					: request.operation === "historyFailure"
						? historyFailure(request.directory)
						: request.operation === "autocomplete"
							? await autocomplete()
							: request.operation === "legacyVariant"
								? legacyVariant(request)
								: dispatch(request);
			process.stdout.write(`${JSON.stringify({ ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		}
	}
}
void main();
