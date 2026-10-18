import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import * as path from "node:path";

type RequestId = string | number | null;

type Request = {
	id: RequestId;
	operation: string;
	[key: string]: unknown;
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function loadApi(sourceDirectory: string, moduleName: string): Promise<Record<string, any>> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

async function searchMatches(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = await loadApi(sourceDirectory, "alt-screen-search.ts");
	const cases = (request.cases ?? []) as Array<{ lines: string[]; query: string }>;
	return {
		results: cases.map((testCase) => ({
			matches: api.findAltScreenSearchMatches(testCase.lines, testCase.query),
			keys: api
				.findAltScreenSearchMatches(testCase.lines, testCase.query)
				.map((match: unknown) => api.getAltScreenSearchMatchKey(match)),
		})),
	};
}

async function searchComponent(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = await loadApi(sourceDirectory, "alt-screen-search.ts");
	const width = Number(request.width ?? 40);
	const queries: string[] = [];
	const renders: string[][] = [];
	const component = new api.AltScreenSearchComponent((query: string) => queries.push(query));
	for (const step of (request.steps ?? []) as Array<Record<string, unknown>>) {
		switch (step.kind) {
			case "render":
				renders.push(component.render(width));
				break;
			case "input":
				component.handleInput(String(step.data ?? ""));
				break;
			case "focus":
				component.focused = Boolean(step.focused);
				break;
			case "result":
				component.setResult(Number(step.index ?? -1), Number(step.count ?? 0));
				break;
			default:
				throw new Error(`unsupported search component step: ${String(step.kind)}`);
		}
	}
	return { renders, queries };
}

class CaptureTerminal {
	columns: number;
	rows: number;
	writes: string[] = [];
	kittyProtocolActive = true;
	private onInput?: (data: string) => void;
	private onResize?: () => void;

	constructor(columns: number, rows: number) {
		this.columns = columns;
		this.rows = rows;
	}

	start(onInput: (data: string) => void, onResize: () => void): void {
		this.onInput = onInput;
		this.onResize = onResize;
		this.write("\x1b[?2004h");
	}

	async drainInput(): Promise<void> {}

	stop(): void {
		this.write("\x1b[?2004l");
		this.onInput = undefined;
		this.onResize = undefined;
	}

	write(data: string): void {
		this.writes.push(data);
	}

	moveBy(lines: number): void {
		if (lines > 0) this.write(`\x1b[${lines}B`);
		else if (lines < 0) this.write(`\x1b[${-lines}A`);
	}

	hideCursor(): void {
		this.write("\x1b[?25l");
	}

	showCursor(): void {
		this.write("\x1b[?25h");
	}

	clearLine(): void {
		this.write("\x1b[K");
	}

	clearFromCursor(): void {
		this.write("\x1b[J");
	}

	clearScreen(): void {
		this.write("\x1b[2J\x1b[H");
	}

	setTitle(title: string): void {
		this.write(`\x1b]0;${title}\x07`);
	}

	setProgress(): void {}

	sendInput(data: string): void {
		this.onInput?.(data);
	}

	resize(columns: number, rows: number): void {
		this.columns = columns;
		this.rows = rows;
		this.onResize?.();
	}
}

class ScenarioComponent {
	lines: string[];
	renderCount = 0;
	inputs: string[] = [];
	readonly focusable: boolean;

	constructor(lines: string[], focusable: boolean) {
		this.lines = lines;
		this.focusable = focusable;
	}

	render(_width: number): string[] {
		this.renderCount += 1;
		return this.lines;
	}

	invalidate(): void {}

	handleInput(data: string): void {
		this.inputs.push(data);
	}
}

class FocusableScenarioComponent extends ScenarioComponent {
	focused = false;
}

async function settle(): Promise<void> {
	await new Promise<void>((resolve) => process.nextTick(resolve));
	await new Promise<void>((resolve) => setTimeout(resolve, 20));
}

type ScenarioStep = Record<string, unknown> & { kind: string };

function stackEntry(components: unknown[], step: Record<string, unknown>): Record<string, unknown> {
	const entry: Record<string, unknown> = { component: components[Number(step.component ?? 0)] };
	if (step.basis !== undefined) entry.basis = step.basis;
	if (step.grow !== undefined) entry.grow = step.grow;
	if (step.shrink !== undefined) entry.shrink = step.shrink;
	if (step.minSize !== undefined) entry.minSize = step.minSize;
	return entry;
}

async function scenario(request: Request, sourceDirectory: string): Promise<unknown> {
	const tuiModule = await loadApi(sourceDirectory, "tui-alt-screen.ts");
	const textModule = await loadApi(sourceDirectory, "components/text.ts");
	const scrollModule = await loadApi(sourceDirectory, "components/scroll-view.ts");
	const vstackModule = await loadApi(sourceDirectory, "components/v-stack.ts");
	const hstackModule = await loadApi(sourceDirectory, "components/h-stack.ts");
	const imageModule = await loadApi(sourceDirectory, "components/image.ts");
	const terminalImage = await loadApi(sourceDirectory, "terminal-image.ts");

	const images = String(request.images ?? "none");
	terminalImage.setCapabilities({
		images: images === "none" ? null : images,
		trueColor: true,
		hyperlinks: true,
	});
	const terminal = new CaptureTerminal(Number(request.columns), Number(request.rows));
	const openedUrls: string[] = [];
	let pasteCount = 0;
	const options: Record<string, unknown> = {};
	const optionInput = (request.options ?? {}) as Record<string, unknown>;
	if (optionInput.wheelScrollLines !== undefined) options.wheelScrollLines = optionInput.wheelScrollLines;
	if (optionInput.mouse !== undefined) options.mouse = optionInput.mouse;
	if (optionInput.customSearchStyles === true) {
		options.searchMatchStyle = (text: string) => `\x1b[41m${text}\x1b[49m`;
		options.searchCurrentMatchStyle = (text: string) => `\x1b[42m${text}\x1b[49m`;
	}
	options.openUrl = (url: string) => openedUrls.push(url);
	options.onRightClickPaste = () => {
		pasteCount += 1;
	};
	const tui = new tuiModule.TuiAltScreen(terminal, undefined, undefined, options);

	const environmentKeys = ["TMUX", "ZELLIJ", "STY", "TERM"] as const;
	const previousEnvironment = new Map<string, string | undefined>(
		environmentKeys.map((key) => [key, process.env[key]]),
	);
	const platformDescriptor = Object.getOwnPropertyDescriptor(process, "platform");
	const environment = (request.environment ?? {}) as Record<string, string>;
	for (const key of environmentKeys) delete process.env[key];
	for (const [key, value] of Object.entries(environment)) {
		if (key === "platform") continue;
		process.env[key] = value;
	}
	if (environment.platform !== undefined) {
		Object.defineProperty(process, "platform", { configurable: true, value: environment.platform });
	}

	const components: unknown[] = [];
	const scrolls: Array<Record<string, any>> = [];
	const texts: Array<Record<string, any>> = [];
	let layoutRoot: unknown;

	try {
		for (const step of (request.steps ?? []) as ScenarioStep[]) {
			switch (step.kind) {
				case "add": {
					const component = step.focusable
						? new FocusableScenarioComponent((step.lines as string[]) ?? [], true)
						: new ScenarioComponent((step.lines as string[]) ?? [], false);
					components.push(component);
					tui.addChild(component);
					break;
				}
				case "add_text": {
					const text = new textModule.Text(
						String(step.text ?? ""),
						Number(step.paddingX ?? 0),
						Number(step.paddingY ?? 0),
					);
					texts.push(text);
					components.push(text);
					tui.addChild(text);
					break;
				}
				case "add_scroll": {
					const scroll = new scrollModule.ScrollView(components[Number(step.component ?? 0)], {
						follow: step.follow ?? undefined,
						primary: Boolean(step.primary),
						scrollbar: step.scrollbar ?? undefined,
						scrollbarHideDelayMs: step.scrollbarHideDelayMs ?? undefined,
					});
					scrolls.push(scroll);
					components.push(scroll);
					tui.addChild(scroll);
					break;
				}
				case "add_vstack": {
					const entries = (step.entries as Record<string, unknown>[]).map((entry) => stackEntry(components, entry));
					const stack = new vstackModule.VStack(entries, { gap: step.gap ?? 0 });
					components.push(stack);
					tui.addChild(stack);
					break;
				}
				case "add_hstack": {
					const entries = (step.entries as Record<string, unknown>[]).map((entry) => stackEntry(components, entry));
					const stack = new hstackModule.HStack(entries, { gap: step.gap ?? 0 });
					components.push(stack);
					tui.addChild(stack);
					break;
				}
				case "add_image": {
					const image = new imageModule.Image(
						String(step.base64Data ?? ""),
						String(step.mimeType ?? "image/png"),
						{ fallbackColor: (value: string) => value },
						{
							...(step.filename === undefined ? {} : { filename: step.filename }),
							...(step.imageId === undefined ? {} : { imageId: Number(step.imageId) }),
						},
						{ widthPx: Number(step.widthPx ?? 800), heightPx: Number(step.heightPx ?? 600) },
					);
					components.push(image);
					tui.addChild(image);
					break;
				}
				case "kitty_metadata":
					terminalImage.registerKittyImageMetadata({
						imageId: Number(step.imageId),
						columns: Number(step.columns),
						rows: Number(step.rows),
						widthPx: Number(step.widthPx),
						heightPx: Number(step.heightPx),
					});
					break;
				case "set_layout_root":
					layoutRoot = step.component === null || step.component === undefined
						? undefined
						: components[Number(step.component)];
					tui.setLayoutRoot(layoutRoot);
					break;
				case "start":
					tui.start();
					break;
				case "wait":
					await settle();
					break;
				case "send_input":
					terminal.sendInput(String(step.data ?? ""));
					break;
				case "set_lines":
					(components[Number(step.component ?? 0)] as ScenarioComponent).lines =
						(step.lines as string[]) ?? [];
					break;
				case "set_text":
					texts[Number(step.index ?? 0)].setText(String(step.text ?? ""));
					break;
				case "request_render":
					tui.requestRender(Boolean(step.force));
					break;
				case "render_now":
					tui.renderNow(Boolean(step.force));
					break;
				case "resize":
					terminal.resize(Number(step.columns ?? request.columns), Number(step.rows ?? request.rows));
					break;
				case "scroll_by":
					tui.scrollBy(Number(step.lines ?? 0));
					break;
				case "scroll_to_top":
					tui.scrollToTop();
					break;
				case "scroll_to_bottom":
					tui.scrollToBottom();
					break;
				case "flash":
					tui.flash(String(step.message ?? ""), step.durationMs as number | undefined);
					break;
				case "focus":
					tui.setFocus(step.component === null || step.component === undefined ? null : components[Number(step.component)]);
					break;
				case "stop":
					tui.stop(step.preserveScreen === undefined ? {} : { preserveScreen: Boolean(step.preserveScreen) });
					break;
				default:
					throw new Error(`unsupported scenario step: ${step.kind}`);
			}
		}

		return {
			writes: terminal.writes.join(""),
			fullRedraws: tui.fullRedraws,
			viewportTop: tui.viewportTop,
			isFollowingOutput: tui.isFollowingOutput,
			scrollTops: scrolls.map((scroll) => scroll.scrollTop),
			followingEnds: scrolls.map((scroll) => scroll.isFollowingEnd),
			scrollbarVisible: scrolls.map((scroll) => scroll.isScrollbarVisible),
			openedUrls,
			pasteCount,
		};
	} finally {
		for (const key of environmentKeys) {
			const value = previousEnvironment.get(key);
			if (value === undefined) delete process.env[key];
			else process.env[key] = value;
		}
		if (platformDescriptor) Object.defineProperty(process, "platform", platformDescriptor);
		terminalImage.resetCapabilitiesCache();
	}
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
		case "search_matches":
			return searchMatches(request, sourceDirectory);
		case "search_component":
			return searchComponent(request, sourceDirectory);
		case "scenario":
			return scenario(request, sourceDirectory);
		default:
			throw new Error(`unsupported operation: ${request.operation}`);
	}
}

async function executeRequestLine(line: string, sourceDirectory: string): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const output = await execute(request, sourceDirectory);
		process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(sourceDirectory: string): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			void executeRequestLine(line, sourceDirectory);
		}
	});
}

function readSourceDirectory(): string {
	const sourceDirectory = process.argv[2];
	if (sourceDirectory === undefined) {
		throw new Error("tui alt-screen oracle requires a source directory path");
	}
	return sourceDirectory;
}

serveRequests(readSourceDirectory());
