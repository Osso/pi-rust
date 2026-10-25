import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [source, root, order] = process.argv.slice(2);
if (!source || !root || !order) throw new Error("source/root/order required");
writeFileSync(join(root, "dialog-stage.txt"), "before import");
const controller = new AbortController();
let ui: {
	theme: unknown;
	setTheme(theme: unknown): unknown;
	getEditorText(): string;
	getAllThemes(): unknown[];
	getTheme(name: string): unknown;
	getToolsExpanded(): boolean;
	setToolsExpanded(expanded: boolean): void;
	getEditorComponent(): unknown;
	setEditorComponent(factory: unknown): void;
	setFooter(factory: unknown): void;
	setDefaultFooter(factory: unknown): void;
	setHeader(factory: unknown): void;
	setWidget(key: string, factory: unknown): void;
	addAutocompleteProvider(factory: unknown): void;
	custom(factory: unknown): Promise<unknown>;
	requestRender(): void;
	setWorkingMessage(message?: string): void;
	setWorkingVisible(visible: boolean): void;
	setWorkingIndicator(options: unknown): void;
	setHiddenThinkingLabel(label: string): void;
	onTerminalInput(handler: unknown): () => void;
	select(
		title: string,
		options: string[],
		opts: { signal: AbortSignal },
	): Promise<string | undefined>;
};
const rawWrite = process.stdout.write.bind(process.stdout);
process.stdout.write = ((
	chunk: string,
	callback?: (error?: Error | null) => void,
) => {
	const request = JSON.parse(chunk) as {
		type: string;
		method?: string;
		id?: string;
	};
	if (request.type === "extension_ui_request" && request.method === "select") {
		const reply = () =>
			process.stdin.emit(
				"data",
				Buffer.from(
					`${JSON.stringify({ type: "extension_ui_response", id: request.id, value: "answered" })}\n`,
				),
			);
		if (order === "response-first") {
			reply();
			controller.abort();
		} else {
			controller.abort();
			reply();
		}
	} else {
		rawWrite(chunk);
	}
	callback?.(null);
	return true;
}) as typeof process.stdout.write;
const session = {
	bindExtensions: async (options: { uiContext: typeof ui }) => {
		ui = options.uiContext;
		writeFileSync(join(root, "dialog-stage.txt"), "UI bound");
	},
	subscribe: () => () => {},
	agent: { subscribe: () => () => {} },
	messages: [],
	consumeResumeContinuationRequest: () => false,
};
const runtime = {
	session,
	setRebindSession: () => {},
	dispose: async () => {},
};
const mode = await import(
	pathToFileURL(join(source, "modes/rpc/rpc-mode.ts")).href
);
setImmediate(async () => {
	writeFileSync(join(root, "dialog-stage.txt"), "observation started");
	if (order === "surface") {
		const forbidden = () => {
			throw new Error("RPC no-op factory must not execute");
		};
		ui.setFooter(forbidden);
		ui.setDefaultFooter(forbidden);
		ui.setHeader(forbidden);
		ui.setWidget("ignored", forbidden);
		ui.addAutocompleteProvider(forbidden);
		ui.setEditorComponent(forbidden);
		ui.requestRender();
		ui.setWorkingMessage("ignored");
		ui.setWorkingVisible(false);
		ui.setWorkingIndicator({});
		ui.setHiddenThinkingLabel("ignored");
		ui.setToolsExpanded(true);
		ui.onTerminalInput(forbidden)();
		const value = {
			stringTheme: ui.setTheme("dark"),
			objectTheme: ui.setTheme(ui.theme),
			editor: ui.getEditorText(),
			themes: ui.getAllThemes(),
			namedTheme: ui.getTheme("dark") ?? null,
			toolsExpanded: ui.getToolsExpanded(),
			editorComponent: ui.getEditorComponent() ?? null,
			custom: (await ui.custom(forbidden)) ?? null,
		};
		writeFileSync(join(root, "dialog-order.json"), JSON.stringify({ value }));
		process.exit(0);
	}
	const value = await ui.select("ordered", ["answered"], {
		signal: controller.signal,
	});
	writeFileSync(
		join(root, "dialog-order.json"),
		JSON.stringify({ value: value ?? null }),
	);
	process.exit(0);
});
await mode.runRpcMode(runtime);
