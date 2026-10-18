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

async function altScreenFlashOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const api = (await loadApi(sourceDirectory, "components/alt-screen-flash.ts")) as {
		AltScreenFlashContainer: new (requestRender: () => void) => LeafComponent & {
			flash(message: string, durationMs?: number): void;
			dispose(): void;
		};
	};
	const cases = requireArray<{ steps: FlashStep[] }>(request, "cases");
	return {
		results: cases.map((testCase) => {
			let renderCount = 0;
			const container = new api.AltScreenFlashContainer(() => {
				renderCount++;
			});
			const snapshots: string[][] = [];
			for (const step of testCase.steps) {
				if (step.kind === "flash") container.flash(step.message, step.durationMs);
				else if (step.kind === "render") snapshots.push(container.render(step.width));
				else container.dispose();
			}
			return { snapshots, renderCount };
		}),
	};
}

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

async function layoutOperation(request: Request, sourceDirectory: string): Promise<unknown> {
	const components = (await loadApi(sourceDirectory, "components/v-stack.ts")) as { VStack: new (children: unknown[], options?: unknown) => unknown };
	const hstackApi = (await loadApi(sourceDirectory, "components/h-stack.ts")) as { HStack: new (children: unknown[], options?: unknown) => unknown };
	const scrollApi = (await loadApi(sourceDirectory, "components/scroll-view.ts")) as { ScrollView: new (component: unknown, options?: unknown) => unknown };
	const textApi = (await loadApi(sourceDirectory, "components/text.ts")) as {
		Text: new (text: string, paddingX?: number, paddingY?: number, bgFn?: (text: string) => string) => unknown;
	};
	const layoutApi = (await loadApi(sourceDirectory, "layout.ts")) as {
		renderLayoutFrame(root: unknown, width: number, height: number, requestRender: () => void): unknown;
	};
	const terminalImage = (await loadApi(sourceDirectory, "terminal-image.ts")) as {
		registerKittyImageMetadata(metadata: Record<string, unknown>): void;
	};

	const byPath = new Map<string, Record<string, unknown>>();

	function buildNode(spec: NodeSpec, pathKey: string): Record<string, unknown> {
		let component: Record<string, unknown>;
		if (spec.kind === "text") {
			const background = spec.background === true;
			component = new textApi.Text(
				String(spec.text),
				Number(spec.paddingX),
				Number(spec.paddingY),
				background ? (text: string) => `[${text}]` : undefined,
			) as unknown as Record<string, unknown>;
		} else if (spec.kind === "lines") {
			if (spec.registerKitty) {
				terminalImage.registerKittyImageMetadata(spec.registerKitty as Record<string, unknown>);
			}
			component = linesComponent(spec.lines as string[]) as unknown as Record<string, unknown>;
		} else if (spec.kind === "vstack" || spec.kind === "hstack") {
			const childSpecs = spec.entries as Array<{ component: NodeSpec; [key: string]: unknown }>;
			const entries = childSpecs.map((entry, index) => {
				const child = buildNode(entry.component, `${pathKey}.${index}`);
				const built: Record<string, unknown> = { component: child };
				if (entry.basis !== undefined && entry.basis !== null) built.basis = entry.basis;
				if (entry.grow !== undefined && entry.grow !== null) built.grow = entry.grow;
				if (entry.shrink !== undefined && entry.shrink !== null) built.shrink = entry.shrink;
				if (entry.minSize !== undefined && entry.minSize !== null) built.minSize = entry.minSize;
				if (entry.maxSize !== undefined && entry.maxSize !== null) built.maxSize = entry.maxSize;
				if (entry.visible === false) built.visible = () => false;
				else if (entry.visible && typeof entry.visible === "object") {
					const minWidth = Number((entry.visible as { minWidth: number }).minWidth);
					built.visible = (viewport: { width: number }) => viewport.width >= minWidth;
				}
				return built;
			});
			const options: Record<string, unknown> = {};
			if (spec.gap !== undefined && spec.gap !== null) options.gap = spec.gap;
			if (spec.align !== undefined && spec.align !== null) options.align = spec.align;
			component =
				spec.kind === "vstack"
					? (new components.VStack(entries, options) as unknown as Record<string, unknown>)
					: (new hstackApi.HStack(entries, options) as unknown as Record<string, unknown>);
		} else if (spec.kind === "scroll") {
			const child = buildNode(spec.component as NodeSpec, `${pathKey}.0`);
			const options: Record<string, unknown> = {};
			if (spec.follow !== undefined && spec.follow !== null) options.follow = spec.follow;
			if (spec.primary !== undefined) options.primary = spec.primary;
			if (spec.overscroll !== undefined && spec.overscroll !== null) options.overscroll = spec.overscroll;
			if (spec.scrollbar !== undefined && spec.scrollbar !== null) options.scrollbar = spec.scrollbar;
			if (spec.scrollbarStyle === "red") {
				options.scrollbarStyle = (text: string) => `\x1b[48;5;1m${text}\x1b[49m`;
			}
			if (spec.scrollbarHideDelayMs !== undefined && spec.scrollbarHideDelayMs !== null) {
				options.scrollbarHideDelayMs = spec.scrollbarHideDelayMs;
			}
			component = new scrollApi.ScrollView(child, options) as unknown as Record<string, unknown>;
		} else {
			throw new Error(`unsupported layout node: ${spec.kind}`);
		}
		byPath.set(pathKey, component);
		return component;
	}

	function serializeFrame(frame: Record<string, unknown>): unknown {
		const boxes: unknown[] = [];
		const scrollViews: unknown[] = [];
		const visit = (box: Record<string, unknown>, pathKey: string): void => {
			const rect = box.rect as { x: number; y: number; width: number; height: number };
			const clip = box.clip as { x: number; y: number; width: number; height: number };
			boxes.push({
				path: pathKey,
				x: rect.x,
				y: rect.y,
				width: rect.width,
				height: rect.height,
				clip: { x: clip.x, y: clip.y, width: clip.width, height: clip.height },
			});
			const scrollView = box.scrollView as
				| {
						scrollTop: number;
						isFollowingEnd: boolean;
						viewportHeight: number;
						isScrollbarVisible: boolean;
						getContentWidth(width: number): number;
				  }
				| undefined;
			if (scrollView) {
				scrollViews.push({
					path: pathKey,
					scrollTop: scrollView.scrollTop,
					followingEnd: scrollView.isFollowingEnd,
					viewportHeight: scrollView.viewportHeight,
					scrollbarVisible: scrollView.isScrollbarVisible,
					contentWidth: scrollView.getContentWidth(rect.width),
				});
			}
			(box.children as Array<Record<string, unknown>>).forEach((child, index) =>
				visit(child, `${pathKey}.${index}`),
			);
		};
		visit(frame.root as Record<string, unknown>, "0");
		return { lines: frame.lines, boxes, scrollViews };
	}

	const cases = requireArray<{ root: NodeSpec; operations: LayoutOp[] }>(request, "cases");
	return {
		results: cases.map((testCase) => {
			byPath.clear();
			const root = buildNode(testCase.root, "0");
			let renderRequests = 0;
			const snapshots: unknown[] = [];
			for (const op of testCase.operations) {
				if (op.kind === "frame") {
					const frame = layoutApi.renderLayoutFrame(root, op.width, op.height, () => {
						renderRequests++;
					});
					snapshots.push(serializeFrame(frame as Record<string, unknown>));
					continue;
				}
				const target = byPath.get(op.target);
				if (!target) throw new Error(`unknown layout target: ${op.target}`);
				if (op.kind === "scrollBy") (target.scrollBy as (lines: number) => number)(op.lines);
				else if (op.kind === "scrollTo") {
					(target.scrollTo as (top: number, options?: unknown) => void)(op.top, {
						disableFollow: op.disableFollow,
					});
				} else if (op.kind === "scrollToEnd") (target.scrollToEnd as () => void)();
				else if (op.kind === "scrollToStart") (target.scrollToStart as () => void)();
				else if (op.kind === "setText") (target.setText as (text: string) => void)(op.text);
				else if (op.kind === "setScrollbar") (target.setScrollbar as (value: string) => void)(op.scrollbar);
			}
			return { snapshots, renderRequests };
		}),
	};
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
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
		case "alt_screen_flash":
			return altScreenFlashOperation(request, sourceDirectory);
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
		case "layout":
			return layoutOperation(request, sourceDirectory);
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
