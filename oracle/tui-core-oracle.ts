import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import * as path from "node:path";
import * as fs from "node:fs";
import * as os from "node:os";

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

async function loadApi(sourceDirectory: string, moduleName: string): Promise<unknown> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

type KeybindingsApi = {
	KeybindingsManager: new (
		definitions: unknown,
		userBindings?: Record<string, string[]>,
	) => {
		getKeys(id: string): string[];
		setUserBindings(bindings: Record<string, string[]>): void;
		getConflicts(): Array<{ key: string; keybindings: string[] }>;
		getResolvedBindings(): Record<string, string | string[]>;
	};
	TUI_KEYBINDINGS: Record<string, unknown>;
};

function toUserBindings(value: unknown): Record<string, string[]> {
	const bindings: Record<string, string[]> = {};
	if (!Array.isArray(value)) return bindings;
	for (const entry of value) {
		if (!Array.isArray(entry) || entry.length !== 2) throw new Error("userBindings entries must be pairs");
		bindings[String(entry[0])] = (entry[1] as unknown[]).map(String);
	}
	return bindings;
}

async function keybindingsOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "keybindings.ts")) as KeybindingsApi;
	const manager = new api.KeybindingsManager(api.TUI_KEYBINDINGS, toUserBindings(request.userBindings));
	const ids = Object.keys(api.TUI_KEYBINDINGS);
	const keys: Record<string, string[]> = {};
	for (const id of ids) keys[id] = manager.getKeys(id);
	const resolved = manager.getResolvedBindings();
	const resolvedArrays: Record<string, string[]> = {};
	for (const id of ids) {
		const value = resolved[id];
		resolvedArrays[id] = Array.isArray(value) ? [...value] : value === undefined ? [] : [value];
	}
	return { keys, resolved: resolvedArrays, conflicts: manager.getConflicts() };
}

async function captureGlobalKeybindings(sourceDirectory: string): Promise<unknown> {
	type Manager = InstanceType<KeybindingsApi["KeybindingsManager"]>;
	const api = await loadApi(sourceDirectory, "keybindings.ts") as KeybindingsApi & {
		getKeybindings(): Manager;
		setKeybindings(manager: Manager): void;
	};
	const installed = new api.KeybindingsManager(api.TUI_KEYBINDINGS);
	api.setKeybindings(installed);
	const first = api.getKeybindings();
	const second = api.getKeybindings();
	first.setUserBindings({ "tui.input.submit": ["ctrl+x"] });
	const shared = [first, second, installed].map((manager) => manager.getKeys("tui.input.submit"));
	installed.setUserBindings({});
	const reset = first.getKeys("tui.input.submit");
	api.setKeybindings(new api.KeybindingsManager(api.TUI_KEYBINDINGS, { "tui.input.submit": ["ctrl+y"] }));
	return { shared, reset, fresh: api.getKeybindings().getKeys("tui.input.submit"), old: first.getKeys("tui.input.submit") };
}

type DebugTui = {
	addChild(component: { render(width: number): string[]; invalidate(): void }): void;
	start(): void;
	stop(): void;
	requestRender(): void;
	doRender(): void;
};

type TerminalApi = {
	normalizeAppleTerminalInput(data: string, apple: boolean, shift: boolean): string;
	parseKeyboardProtocolNegotiationSequence(
		sequence: string,
	): { type: string; flags?: number } | undefined;
	isTerminalRawModeFailure(error: Error): boolean;
	TerminalRawModeError: new (flag: boolean, cause: unknown) => Error;
	ProcessTerminal: new () => { setProgress(active: boolean): void };
};

async function terminalOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "terminal.ts")) as TerminalApi;
	const sequences = (request.sequences ?? []) as string[];
	const rawModeCases = (request.rawModeErrors ?? []) as Array<{ flag: boolean; message: string }>;
	const normalizeCases = (request.normalize ?? []) as Array<{
		data: string;
		flag: boolean;
		shift: boolean;
		kind: "native" | "apple";
	}>;
	const terminal = new api.ProcessTerminal();
	const writes: string[] = [];
	const write = process.stdout.write;
	try {
		process.stdout.write = ((data: string) => { writes.push(data); return true; }) as typeof write;
		terminal.setProgress(true);
		terminal.setProgress(false);
	} finally {
		process.stdout.write = write;
	}
	return {
		progress: writes.join(""),
		sequences: sequences.map((sequence) => api.parseKeyboardProtocolNegotiationSequence(sequence) ?? null),
		normalize: normalizeCases.map((testCase) =>
			api.normalizeAppleTerminalInput(testCase.data, testCase.flag, testCase.shift),
		),
		rawMode: rawModeCases.map((testCase) => {
			const typed = new api.TerminalRawModeError(testCase.flag, new Error(testCase.message));
			return {
				message: typed.message,
				typed: api.isTerminalRawModeFailure(typed),
				plain: api.isTerminalRawModeFailure(new Error(testCase.message)),
			};
		}),
	};
}


type ScenarioStep = {
	kind: string;
	component?: number;
	lines?: string[];
	force?: boolean;
	data?: string;
	columns?: number;
	rows?: number;
	options?: Record<string, unknown>;
	place?: [number, number];
	update?: string;
};

type ScenarioRequest = {
	kind: "main" | "renderer";
	columns: number;
	rows: number;
	showHardwareCursor?: boolean;
	clearOnShrink?: boolean;
	termux?: boolean;
	steps: ScenarioStep[];
};

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
	readonly focusOnInput: Map<string, number>;
	private readonly components: ScenarioComponent[];
	private readonly tuiRef: { current?: { setFocus(component: unknown): void } };

	constructor(
		lines: string[],
		focusable: boolean,
		focusOnInput: Map<string, number>,
		components: ScenarioComponent[],
		tuiRef: { current?: { setFocus(component: unknown): void } },
	) {
		this.lines = lines;
		this.focusable = focusable;
		this.focusOnInput = focusOnInput;
		this.components = components;
		this.tuiRef = tuiRef;
	}

	render(_width: number): string[] {
		this.renderCount += 1;
		return this.lines;
	}

	invalidate(): void {}

	handleInput(data: string): void {
		this.inputs.push(data);
		const target = this.focusOnInput.get(data);
		if (target !== undefined) this.tuiRef.current?.setFocus(this.components[target]);
	}
}

class FocusableScenarioComponent extends ScenarioComponent {
	focused = false;
}

async function settle(): Promise<void> {
	await new Promise<void>((resolve) => process.nextTick(resolve));
	await new Promise<void>((resolve) => setTimeout(resolve, 20));
}

function overlayOptions(value: Record<string, unknown>): unknown {
	const options: Record<string, unknown> = {};
	if (value.width !== undefined) options.width = value.width;
	if (value.minWidth !== undefined) options.minWidth = value.minWidth;
	if (value.maxHeight !== undefined) options.maxHeight = value.maxHeight;
	if (value.anchor !== undefined) options.anchor = value.anchor;
	if (value.offsetX !== undefined) options.offsetX = value.offsetX;
	if (value.offsetY !== undefined) options.offsetY = value.offsetY;
	if (value.row !== undefined) options.row = value.row;
	if (value.col !== undefined) options.col = value.col;
	if (value.margin !== undefined) options.margin = value.margin;
	if (value.nonCapturing !== undefined) options.nonCapturing = value.nonCapturing;
	return options;
}

async function tuiScenario(request: ScenarioRequest, sourceDirectory: string): Promise<unknown> {
	const tuiModule = (await loadApi(sourceDirectory, "tui.ts")) as Record<string, any>;
	const terminal = new CaptureTerminal(request.columns, request.rows);
	const tui = new tuiModule.TUI(terminal);
	if (request.showHardwareCursor !== undefined) tui.setShowHardwareCursor(request.showHardwareCursor);
	if (request.clearOnShrink !== undefined) tui.setClearOnShrink(request.clearOnShrink);
	if (request.termux === true) process.env.TERMUX_VERSION = "1";
	if (request.termux === false) delete process.env.TERMUX_VERSION;
	const tuiRef: { current?: { setFocus(component: unknown): void } } = { current: tui };
	const components: ScenarioComponent[] = [];
	const overlayHandles: Array<{ hide(): void }> = [];
	const flowRegions: Array<{ place(layout: unknown): void; requestRender(): boolean }> = [];
	const flowResults: boolean[] = [];
	const backgroundQueries: Array<Promise<unknown>> = [];
	const fixedCells: Array<{ place(row: number, col: number): void; update(value: string): boolean }> = [];
	let nextComponent = 0;

	for (const step of request.steps) {
		switch (step.kind) {
			case "add": {
				const focusOnInput = new Map<string, number>();
				const bindings = (step as unknown as { focusOnInput?: Array<[string, number]> }).focusOnInput ?? [];
				for (const [key, target] of bindings) focusOnInput.set(key, target);
				const focusable = Boolean((step as unknown as { focusable?: boolean }).focusable);
				const component = focusable
					? new FocusableScenarioComponent(step.lines ?? [], true, focusOnInput, components, tuiRef)
					: new ScenarioComponent(step.lines ?? [], false, focusOnInput, components, tuiRef);
				components.push(component);
				tui.addChild(component);
				nextComponent += 1;
				break;
			}
			case "start":
				tui.start();
				break;
			case "wait":
				await settle();
				break;
			case "render_now":
				tui.requestRender(Boolean(step.force));
				await settle();
				break;
			case "request_render":
				tui.requestRender(Boolean(step.force));
				break;
			case "set_lines":
				components[step.component ?? 0].lines = step.lines ?? [];
				break;
			case "send_input":
				terminal.sendInput(step.data ?? "");
				break;
			case "focus":
				tui.setFocus(step.component === undefined || step.component === null ? null : components[step.component]);
				break;
			case "resize":
				terminal.resize(step.columns ?? request.columns, step.rows ?? request.rows);
				break;
			case "overlay": {
				const handle = tui.showOverlay(components[step.component ?? 0], overlayOptions(step.options ?? {}));
				overlayHandles.push(handle);
				break;
			}
			case "hide_overlay":
				tui.hideOverlay();
				break;
			case "overlay_op": {
				const handle = overlayHandles[(step as unknown as { index?: number }).index ?? 0];
				const operation = (step as unknown as { operation?: string }).operation;
				if (operation === "hide") handle.hide();
				break;
			}
			case "fixed_cell_create":
				fixedCells.push(tui.createFixedCell());
				break;
			case "fixed_cell_place": {
				const cell = fixedCells[(step as unknown as { index?: number }).index ?? 0];
				const [row, col] = step.place ?? [0, 0];
				cell.place(row, col);
				break;
			}
			case "fixed_cell_update": {
				const cell = fixedCells[(step as unknown as { index?: number }).index ?? 0];
				(step as unknown as { result?: boolean }).result = cell.update(step.update ?? "");
				break;
			}
			case "stop":
				tui.stop();
				break;
			case "flow_create":
				flowRegions.push(tui.createFlowRenderRegion(components[step.component ?? 0]));
				break;
			case "flow_place":
				flowRegions[0].place((step as unknown as { layout: unknown }).layout);
				break;
			case "flow_request":
				flowResults.push(flowRegions[0].requestRender());
				break;
			case "query_background":
				backgroundQueries.push(tui.queryTerminalBackgroundColor({ timeoutMs: 0 }));
				break;
			case "snapshot":
				break;
			default:
				throw new Error(`unsupported scenario step: ${step.kind}`);
		}
	}
	if (request.termux !== undefined) delete process.env.TERMUX_VERSION;
	return {
		writes: terminal.writes.join(""),
		fullRedraws: tui.fullRedraws,
		components: components.map((component) => ({
			renders: component.renderCount,
			inputs: component.inputs,
			focused: Boolean((component as unknown as { focused?: boolean }).focused),
		})),
		background: await Promise.all(backgroundQueries),
		flowResults,
		focus: components.findIndex((component) => Boolean((component as unknown as { focused?: boolean }).focused)),
	};
}

async function captureTerminalWriteLog(sourceDirectory: string): Promise<unknown> {
	const api = await loadApi(sourceDirectory, "terminal.ts") as { ProcessTerminal: new () => { write(data: string): void } };
	const directory = fs.mkdtempSync(path.join(os.tmpdir(), "write-log-"));
	const write = process.stdout.write;
	process.env.PI_TUI_WRITE_LOG = directory;
	try {
		process.stdout.write = (() => true) as typeof write;
		new api.ProcessTerminal().write("payload\n");
		const filename = fs.readdirSync(directory)[0];
		const normalized = filename.replace(/\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}/, "TIME").replace(`-${process.pid}.log`, "-PID.log");
		const contents = fs.readFileSync(path.join(directory, filename), "utf8");
		const missing = path.join(directory, "absent", "log.txt");
		process.env.PI_TUI_WRITE_LOG = missing;
		new api.ProcessTerminal().write("ignored log error");
		return { normalized, contents, createdMissingParent: fs.existsSync(path.dirname(missing)) };
	} finally {
		process.stdout.write = write;
		delete process.env.PI_TUI_WRITE_LOG;
	}
}

async function captureTuiDebugFailure(request: Request, sourceDirectory: string): Promise<unknown> {
	const { TUI } = await loadApi(sourceDirectory, "tui.ts") as { TUI: new (terminal: CaptureTerminal) => DebugTui };
	process.env.HOME = request.home as string;
	process.env.PI_DEBUG_REDRAW = "1";
	const tui = new TUI(new CaptureTerminal(20, 5));
	tui.addChild({ render: () => ["first"], invalidate: () => {} });
	try {
		tui.doRender();
		return { failed: false };
	} catch {
		return { failed: true };
	}
}

async function captureTuiDebug(sourceDirectory: string): Promise<unknown> {
	const { TUI } = await loadApi(sourceDirectory, "tui.ts") as { TUI: new (terminal: CaptureTerminal) => DebugTui };
	const logDirectory = path.join(os.homedir(), ".pi", "agent");
	fs.mkdirSync(logDirectory, { recursive: true });
	process.env.PI_CODING_AGENT_DIR = path.join(os.homedir(), "wrong-agent-dir");
	let lines = ["first"];
	const terminal = new CaptureTerminal(20, 5);
	const tui = new TUI(terminal);
	process.env.PI_DEBUG_REDRAW = "1";
	process.env.PI_TUI_DEBUG = "1";
	tui.addChild({ render: () => lines, invalidate: () => {} });
	tui.start();
	await settle();
	lines = ["changed"];
	tui.requestRender();
	await settle();
	tui.stop();
	const debug = fs.readFileSync(path.join(logDirectory, "pi-debug.log"), "utf8").replace(/\[[^\]]+\]/g, "[TIME]");
	const dumps = fs.readdirSync("/tmp/tui").map((name) => fs.readFileSync(path.join("/tmp/tui", name), "utf8")).sort();
	return { debug, dumps };
}

async function captureTerminalNumericDomains(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = await loadApi(sourceDirectory, "terminal.ts") as {
		ProcessTerminal: new () => { columns: number; rows: number };
		parseKeyboardProtocolNegotiationSequence(sequence: string): unknown;
	};
	const previousColumns = process.env.COLUMNS;
	const previousLines = process.env.LINES;
	try {
		const terminal = new api.ProcessTerminal();
		const dimensions = (request.values as string[]).map((value) => {
			process.env.COLUMNS = value;
			process.env.LINES = value;
			return { input: value, columns: String(terminal.columns), rows: String(terminal.rows) };
		});
		return { dimensions, hugeFlags: api.parseKeyboardProtocolNegotiationSequence("\x1b[?" + "9".repeat(400) + "u") };
	} finally {
		if (previousColumns === undefined) delete process.env.COLUMNS;
		else process.env.COLUMNS = previousColumns;
		if (previousLines === undefined) delete process.env.LINES;
		else process.env.LINES = previousLines;
	}
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
		case "keybinding_global":
			return captureGlobalKeybindings(sourceDirectory);
		case "terminal_write_log":
			return captureTerminalWriteLog(sourceDirectory);
		case "tui_debug_failure":
			return captureTuiDebugFailure(request, sourceDirectory);
		case "tui_debug":
			return captureTuiDebug(sourceDirectory);
		case "terminal_numeric_domains":
			return captureTerminalNumericDomains(request, sourceDirectory);
		case "keybindings":
			return keybindingsOperation(request, sourceDirectory);
		case "terminal":
			return terminalOperation(request, sourceDirectory);
		case "tui_scenario":
			return tuiScenario(request as unknown as ScenarioRequest, sourceDirectory);
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
		throw new Error("tui core oracle requires a source directory path");
	}
	return sourceDirectory;
}

serveRequests(readSourceDirectory());
