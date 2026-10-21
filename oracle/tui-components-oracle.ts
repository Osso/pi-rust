import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import path from "node:path";

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
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function requireArray<T>(request: Request, key: string): T[] {
	const value = request[key];
	if (!Array.isArray(value)) throw new Error(`${request.operation} requires array field ${key}`);
	return value as T[];
}

async function loadApi(sourceDirectory: string, moduleName: string): Promise<Record<string, unknown>> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

function nullable<T>(value: T | undefined | null): T | null {
	return value === undefined || value === null ? null : value;
}

function identity(text: string): string {
	return text;
}

function markerStyle(text: string): string {
	return `«${text}»`;
}

type LeafComponent = {
	render(width: number): string[];
	handleInput?(data: string): void;
	invalidate?(): void;
};

function linesComponent(lines: string[]): LeafComponent {
	return { render: () => lines, invalidate: () => {} };
}

async function textOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/text.ts")) as {
		Text: new (text: string, paddingX?: number, paddingY?: number, bgFn?: (text: string) => string) => LeafComponent;
	};
	const cases = requireArray<{
		text: string;
		paddingX: number;
		paddingY: number;
		background: boolean;
		width: number;
	}>(request, "cases");
	return {
		results: cases.map((testCase) => {
			const bgFn = testCase.background ? (text: string) => `[${text}]` : undefined;
			const text = new api.Text(testCase.text, testCase.paddingX, testCase.paddingY, bgFn);
			return text.render(testCase.width);
		}),
	};
}

type LoaderStep =
	| { kind: "render"; width: number }
	| { kind: "setMessage"; message: string }
	| { kind: "setIndicator"; frames: string[] | null; intervalMs: number | null }
	| { kind: "stop" };

async function loaderOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/loader.ts")) as {
		Loader: new (
			ui: { requestComponentRender(): boolean },
			spinnerColorFn: (text: string) => string,
			messageColorFn: (text: string) => string,
			message?: string,
			indicator?: { frames?: string[]; intervalMs?: number },
		) => LeafComponent & {
			start(): void;
			stop(): void;
			setMessage(message: string): void;
			setIndicator(indicator?: { frames?: string[]; intervalMs?: number }): void;
		};
	};
	const cases = requireArray<{
		message: string;
		frames: string[] | null;
		intervalMs: number | null;
		styled: boolean;
		steps: LoaderStep[];
	}>(request, "cases");
	return {
		results: cases.map((testCase) => {
			let renderCount = 0;
			const ui = {
				requestComponentRender(): boolean {
					renderCount++;
					return false;
				},
			};
			const spinner = testCase.styled ? (text: string) => `<${text}>` : identity;
			const messageStyle = testCase.styled ? (text: string) => `{${text}}` : identity;
			const indicator =
				testCase.frames === null && testCase.intervalMs === null
					? undefined
					: {
							...(testCase.frames === null ? {} : { frames: testCase.frames }),
							...(testCase.intervalMs === null ? {} : { intervalMs: testCase.intervalMs }),
						};
			const loader = new api.Loader(ui, spinner, messageStyle, testCase.message, indicator);
			const snapshots: string[][] = [];
			try {
				for (const step of testCase.steps) {
					if (step.kind === "render") snapshots.push(loader.render(step.width));
					else if (step.kind === "setMessage") loader.setMessage(step.message);
					else if (step.kind === "setIndicator") {
						loader.setIndicator(
							step.frames === null && step.intervalMs === null
								? undefined
								: {
										...(step.frames === null ? {} : { frames: step.frames }),
										...(step.intervalMs === null ? {} : { intervalMs: step.intervalMs }),
									},
						);
					} else loader.stop();
				}
			} finally {
				loader.stop();
			}
			return { snapshots, renderCount };
		}),
	};
}

type SelectListStep =
	| { kind: "filter"; filter: string }
	| { kind: "index"; index: number }
	| { kind: "key"; data: string }
	| { kind: "render"; width: number };

type SelectListItem = { value: string; label: string; description?: string };

async function selectListOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/select-list.ts")) as {
		SelectList: new (
			items: SelectListItem[],
			maxVisible: number,
			theme: Record<string, (text: string) => string>,
			layout?: Record<string, unknown>,
		) => LeafComponent & {
			onSelect?: (item: SelectListItem) => void;
			onCancel?: () => void;
			onSelectionChange?: (item: SelectListItem) => void;
			setFilter(filter: string): void;
			setSelectedIndex(index: number): void;
			getSelectedItem(): SelectListItem | null;
		};
	};
	const cases = requireArray<{
		items: SelectListItem[];
		maxVisible: number;
		markers: boolean;
		layout: { minPrimaryColumnWidth: number | null; maxPrimaryColumnWidth: number | null; truncatePrimary: boolean };
		steps: SelectListStep[];
	}>(request, "cases");
	return {
		results: cases.map((testCase) => {
			const style = testCase.markers ? markerStyle : identity;
			const theme = {
				selectedPrefix: style,
				selectedText: style,
				description: style,
				scrollInfo: style,
				noMatch: style,
			};
			const layout: Record<string, unknown> = {};
			if (testCase.layout.minPrimaryColumnWidth !== null) {
				layout.minPrimaryColumnWidth = testCase.layout.minPrimaryColumnWidth;
			}
			if (testCase.layout.maxPrimaryColumnWidth !== null) {
				layout.maxPrimaryColumnWidth = testCase.layout.maxPrimaryColumnWidth;
			}
			if (testCase.layout.truncatePrimary) {
				layout.truncatePrimary = (context: { text: string; maxWidth: number }) =>
					context.text.length <= context.maxWidth
						? context.text
						: `${context.text.slice(0, Math.max(0, context.maxWidth - 1))}…`;
			}
			const list = new api.SelectList(testCase.items, testCase.maxVisible, theme, layout);
			const log: string[] = [];
			list.onSelect = (item) => log.push(`select:${item.value}`);
			list.onCancel = () => log.push("cancel");
			list.onSelectionChange = (item) => log.push(`change:${item.value}`);
			const snapshots: string[][] = [];
			for (const step of testCase.steps) {
				if (step.kind === "filter") list.setFilter(step.filter);
				else if (step.kind === "index") list.setSelectedIndex(step.index);
				else if (step.kind === "key") list.handleInput(step.data);
				else snapshots.push(list.render(step.width));
			}
			return {
				snapshots,
				log,
				selected: list.getSelectedItem()?.value ?? null,
			};
		}),
	};
}

type InputStep =
	| { kind: "setValue"; value: string }
	| { kind: "key"; data: string }
	| { kind: "render"; width: number }
	| { kind: "value" };

async function inputOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/input.ts")) as {
		Input: new (options?: { mask?: boolean }) => LeafComponent & {
			focused: boolean;
			onSubmit?: (value: string) => void;
			onEscape?: () => void;
			getValue(): string;
			setValue(value: string): void;
		};
	};
	const cases = requireArray<{ mask: boolean; focused: boolean; steps: InputStep[] }>(request, "cases");
	return {
		results: cases.map((testCase) => {
			const input = new api.Input({ mask: testCase.mask });
			input.focused = testCase.focused;
			const submitted: string[] = [];
			let escapes = 0;
			input.onSubmit = (value) => submitted.push(value);
			input.onEscape = () => {
				escapes++;
			};
			const lines: string[][] = [];
			const values: string[] = [];
			for (const step of testCase.steps) {
				if (step.kind === "setValue") input.setValue(step.value);
				else if (step.kind === "key") input.handleInput(step.data);
				else if (step.kind === "render") lines.push(input.render(step.width));
				else values.push(input.getValue());
			}
			return { lines, values, submitted, escapes };
		}),
	};
}

type SettingsListStep =
	| { kind: "key"; data: string }
	| { kind: "render"; width: number }
	| { kind: "updateValue"; id: string; value: string };

async function settingsListOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/settings-list.ts")) as {
		SettingsList: new (
			items: unknown[],
			maxVisible: number,
			theme: Record<string, unknown>,
			onChange: (id: string, value: string) => void,
			onCancel: () => void,
			options?: { enableSearch?: boolean },
		) => LeafComponent & { updateValue(id: string, value: string): void };
	};
	const cases = requireArray<{
		items: Array<{
			id: string;
			label: string;
			description?: string;
			currentValue: string;
			values?: string[];
			submenu?: { options: string[] };
		}>;
		maxVisible: number;
		enableSearch: boolean;
		markers: boolean;
		steps: SettingsListStep[];
	}>(request, "cases");
	return {
		results: cases.map((testCase) => {
			const style = testCase.markers ? markerStyle : identity;
			const theme = {
				label: (text: string, selected: boolean) => (testCase.markers ? `L${selected ? "+" : "-"}${text}` : text),
				value: (text: string, selected: boolean) => (testCase.markers ? `V${selected ? "+" : "-"}${text}` : text),
				description: style,
				cursor: "> ",
				hint: style,
			};
			const changes: string[] = [];
			let cancels = 0;
			const items = testCase.items.map((item) => ({
				...item,
				...(item.submenu
					? {
							submenu: (currentValue: string, done: (value?: string) => void): LeafComponent => ({
								render: () => [`submenu:${currentValue}`],
								handleInput: (data: string) => {
									if (data === "\r") done(item.submenu!.options[0]);
									else if (data === "\x1b") done(undefined);
								},
								invalidate: () => {},
							}),
						}
					: {}),
			}));
			const list = new api.SettingsList(
				items,
				testCase.maxVisible,
				theme,
				(id, value) => changes.push(`${id}=${value}`),
				() => {
					cancels++;
				},
				{ enableSearch: testCase.enableSearch },
			);
			const snapshots: string[][] = [];
			for (const step of testCase.steps) {
				if (step.kind === "key") list.handleInput(step.data);
				else if (step.kind === "render") snapshots.push(list.render(step.width));
				else list.updateValue(step.id, step.value);
			}
			return { snapshots, changes, cancels };
		}),
	};
}

type FlashStep =
	| { kind: "flash"; message: string; durationMs: number }
	| { kind: "render"; width: number }
	| { kind: "dispose" };

async function cancellableLoaderOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/cancellable-loader.ts")) as {
		CancellableLoader: new (
			ui: { requestComponentRender(): boolean },
			spinnerColorFn: (text: string) => string,
			messageColorFn: (text: string) => string,
			message?: string,
			indicator?: { frames?: string[]; intervalMs?: number },
		) => LeafComponent & {
			onAbort?: () => void;
			aborted: boolean;
			stop(): void;
			setMessage(message: string): void;
		};
	};
	const cases = requireArray<{
		message: string;
		frames: string[] | null;
		steps: Array<{ kind: "key"; data: string } | { kind: "render"; width: number } | { kind: "setMessage"; message: string }>;
	}>(request, "cases");
	return {
		results: cases.map((testCase) => {
			let renderCount = 0;
			const ui = {
				requestComponentRender(): boolean {
					renderCount++;
					return false;
				},
			};
			const indicator =
				testCase.frames === null ? undefined : { frames: testCase.frames, intervalMs: 250 };
			const loader = new api.CancellableLoader(ui, identity, identity, testCase.message, indicator);
			const log: string[] = [];
			loader.onAbort = () => log.push("abort");
			const snapshots: string[][] = [];
			try {
				for (const step of testCase.steps) {
					if (step.kind === "key") loader.handleInput(step.data);
					else if (step.kind === "render") snapshots.push(loader.render(step.width));
					else loader.setMessage(step.message);
				}
			} finally {
				loader.stop();
			}
			return { snapshots, log, aborted: loader.aborted, renderCount };
		}),
	};
}

async function imageOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const imageApi = (await loadApi(sourceDirectory, "terminal-image.ts")) as {
		setCapabilities(caps: { images: "kitty" | "iterm2" | null; trueColor: boolean; hyperlinks: boolean }): void;
		resetCapabilitiesCache(): void;
		setCellDimensions(dims: { widthPx: number; heightPx: number }): void;
	};
	const componentApi = (await loadApi(sourceDirectory, "components/image.ts")) as {
		Image: new (
			base64Data: string,
			mimeType: string,
			theme: { fallbackColor: (text: string) => string },
			options: Record<string, unknown>,
			dimensions?: { widthPx: number; heightPx: number },
		) => LeafComponent & { getImageId(): number | undefined };
	};
	const cases = requireArray<{
		images: "kitty" | "iterm2" | null;
		cellWidthPx: number;
		cellHeightPx: number;
		base64Data: string;
		mimeType: string;
		dimensions: { widthPx: number; heightPx: number };
		options: {
			maxWidthCells: number | null;
			maxHeightCells: number | null;
			filename: string | null;
			imageId: number | null;
		};
		width: number;
	}>(request, "cases");
	return {
		results: cases.map((testCase) => {
			imageApi.setCapabilities({ images: testCase.images, trueColor: true, hyperlinks: true });
			imageApi.setCellDimensions({ widthPx: testCase.cellWidthPx, heightPx: testCase.cellHeightPx });
			try {
				const options: Record<string, unknown> = {};
				if (testCase.options.maxWidthCells !== null) options.maxWidthCells = testCase.options.maxWidthCells;
				if (testCase.options.maxHeightCells !== null) options.maxHeightCells = testCase.options.maxHeightCells;
				if (testCase.options.filename !== null) options.filename = testCase.options.filename;
				if (testCase.options.imageId !== null) options.imageId = testCase.options.imageId;
				const image = new componentApi.Image(
					testCase.base64Data,
					testCase.mimeType,
					{ fallbackColor: (text: string) => `<${text}>` },
					options,
					testCase.dimensions,
				);
				return { lines: image.render(testCase.width), imageId: nullable(image.getImageId()) };
			} finally {
				imageApi.resetCapabilitiesCache();
			}
		}),
	};
}

async function boxOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/box.ts")) as {
		Box: new (paddingX?: number, paddingY?: number, bgFn?: (text: string) => string) => LeafComponent & {
			addChild(component: LeafComponent): void;
			setBgFn(bgFn?: (text: string) => string): void;
		};
	};
	const cases = requireArray<{
		paddingX: number;
		paddingY: number;
		background: boolean;
		childLines: string[];
		width: number;
		setBackgroundAfterFirstRender: boolean;
	}>(request, "cases");
	return {
		results: cases.map((testCase) => {
			const bgFn = testCase.background ? (text: string) => `[${text}]` : undefined;
			const box = new api.Box(testCase.paddingX, testCase.paddingY, bgFn);
			box.addChild(linesComponent(testCase.childLines));
			const first = box.render(testCase.width);
			if (!testCase.setBackgroundAfterFirstRender) return { snapshots: [first] };
			box.setBgFn((text: string) => `<${text}>`);
			return { snapshots: [first, box.render(testCase.width)] };
		}),
	};
}

async function spacerOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/spacer.ts")) as {
		Spacer: new (lines?: number) => LeafComponent & { setLines(lines: number): void };
	};
	const cases = requireArray<{ lines: number; width: number; setLines: number | null }>(request, "cases");
	return {
		results: cases.map((testCase) => {
			const spacer = new api.Spacer(testCase.lines);
			const snapshots = [spacer.render(testCase.width)];
			if (testCase.setLines !== null) {
				spacer.setLines(testCase.setLines);
				snapshots.push(spacer.render(testCase.width));
			}
			return { snapshots };
		}),
	};
}

async function dynamicBorderOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/dynamic-border.ts")) as {
		DynamicBorder: new (format?: (line: string) => string) => LeafComponent;
	};
	const cases = requireArray<{ width: number; markers: boolean }>(request, "cases");
	return {
		results: cases.map((testCase) => {
			const border = new api.DynamicBorder(testCase.markers ? (line: string) => `[${line}]` : undefined);
			return border.render(testCase.width);
		}),
	};
}

async function truncatedTextOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/truncated-text.ts")) as {
		TruncatedText: new (text: string, paddingX?: number, paddingY?: number) => LeafComponent;
	};
	const cases = requireArray<{ text: string; paddingX: number; paddingY: number; width: number }>(
		request,
		"cases",
	);
	return {
		results: cases.map((testCase) => {
			const text = new api.TruncatedText(testCase.text, testCase.paddingX, testCase.paddingY);
			return text.render(testCase.width);
		}),
	};
}

type NodeSpec = {
	kind: string;
	[key: string]: unknown;
};

type LayoutOp =
	| { kind: "frame"; width: number; height: number }
	| { kind: "scrollBy"; target: string; lines: number }
	| { kind: "scrollTo"; target: string; top: number; disableFollow: boolean }
	| { kind: "scrollToEnd"; target: string }
	| { kind: "scrollToStart"; target: string }
	| { kind: "setText"; target: string; text: string }
	| { kind: "setScrollbar"; target: string; scrollbar: "hidden" | "auto" | "always" };

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
        case "select_list_signed": {
            const { SelectList } = await loadApi(sourceDirectory, "components/select-list.ts") as { SelectList: any };
            const calls: Array<[number, number]> = [];
            const list = new SelectList([{ value: "alpha", label: "alpha" }], 5,
                { selectedPrefix: identity, selectedText: identity, description: identity, scrollInfo: identity, noMatch: identity },
                { truncatePrimary: (context: { maxWidth: number; columnWidth: number; text: string }) => {
                    calls.push([context.maxWidth, context.columnWidth]);
                    return context.text;
                } });
            return { lines: [0, 1, 2, 3, 4].map(width => list.render(width)), calls };
        }
		case "text":
			return textOperation(request, sourceDirectory);
		case "loader":
			return loaderOperation(request, sourceDirectory);
		case "select_list":
			return selectListOperation(request, sourceDirectory);
		case "input":
			return inputOperation(request, sourceDirectory);
		case "settings_list":
			return settingsListOperation(request, sourceDirectory);
		case "cancellable_loader":
			return cancellableLoaderOperation(request, sourceDirectory);
		case "image":
			return imageOperation(request, sourceDirectory);
		case "box":
			return boxOperation(request, sourceDirectory);
		case "spacer":
			return spacerOperation(request, sourceDirectory);
		case "dynamic_border":
			return dynamicBorderOperation(request, sourceDirectory);
		case "truncated_text":
			return truncatedTextOperation(request, sourceDirectory);
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
		const response: Response = { id, ok: true, output };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
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
		throw new Error("tui components oracle requires a source directory path");
	}
	return sourceDirectory;
}

serveRequests(readSourceDirectory());
