import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import * as path from "node:path";

type RequestId = string | number | null;

type FuzzyMatchApi = {
	fuzzyMatch(query: string, text: string): { matches: boolean; score: number };
	fuzzyFilter<T>(items: T[], query: string, getText: (item: T) => string | string[]): T[];
};

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

function requireString(request: Request, key: string): string {
	const value = request[key];
	if (typeof value !== "string") throw new Error(`${request.operation} requires string field ${key}`);
	return value;
}

function requireArray<T>(request: Request, key: string): T[] {
	const value = request[key];
	if (!Array.isArray(value)) throw new Error(`${request.operation} requires array field ${key}`);
	return value as T[];
}

function f64Bits(value: number): string {
	const buffer = new ArrayBuffer(8);
	new DataView(buffer).setFloat64(0, value, false);
	return Array.from(new Uint8Array(buffer), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

type KillRingApi = {
	KillRing: new () => {
		push(text: string, opts: { prepend: boolean; accumulate?: boolean }): void;
		peek(): string | undefined;
		rotate(): void;
		readonly length: number;
	};
};

type KillRingOperation =
	| { kind: "push"; text: string; prepend: boolean; accumulate: boolean }
	| { kind: "rotate" }
	| { kind: "peek" };

function simulateKillRing(api: KillRingApi, operations: KillRingOperation[]): unknown {
	const ring = new api.KillRing();
	const snapshots: unknown[] = [];
	for (const operation of operations) {
		if (operation.kind === "push") {
			ring.push(operation.text, { prepend: operation.prepend, accumulate: operation.accumulate });
		} else if (operation.kind === "rotate") {
			ring.rotate();
		} else {
			snapshots.push({ peek: ring.peek() ?? null, length: ring.length });
		}
	}
	const entries: Array<string | null> = [];
	for (let index = 0; index < ring.length; index++) {
		entries.push(ring.peek() ?? null);
		ring.rotate();
	}
	return { snapshots, length: ring.length, entries };
}

type TerminalColorsApi = {
	parseOsc11BackgroundColor(data: string): { r: number; g: number; b: number } | undefined;
	isOsc11BackgroundColorResponse(data: string): boolean;
	parseTerminalColorSchemeReport(data: string): "dark" | "light" | undefined;
};

type UtilsApi = {
	visibleWidth(value: string): number;
	normalizeTerminalOutput(value: string): string;
	truncateToWidth(text: string, maxWidth: number, ellipsis: string, pad: boolean): string;
	wrapTextWithAnsi(text: string, width: number): string[];
	sliceWithWidth(line: string, startCol: number, length: number, strict: boolean): { text: string; width: number };
	extractSegments(
		line: string,
		beforeEnd: number,
		afterStart: number,
		afterLen: number,
		strictAfter: boolean,
	): { before: string; beforeWidth: number; after: string; afterWidth: number };
	extractAnsiCode(value: string, pos: number): { code: string; length: number } | null;
	applyBackgroundToLine(line: string, width: number, bgFn: (text: string) => string): string;
	isWhitespaceChar(value: string): boolean;
	isPunctuationChar(value: string): boolean;
};

function scanAnsiCodes(api: UtilsApi, value: string): string[] {
	const codes: string[] = [];
	let index = 0;
	while (index < value.length) {
		const ansi = api.extractAnsiCode(value, index);
		if (ansi) {
			codes.push(ansi.code);
			index += ansi.length;
		} else {
			index++;
		}
	}
	return codes;
}

type KeysApi = {
	setKittyProtocolActive(active: boolean): void;
	parseKey(data: string): string | undefined;
	matchesKey(data: string, keyId: string): boolean;
	decodeKittyPrintable(data: string): string | undefined;
	decodePrintableKey(data: string): string | undefined;
	isKeyRelease(data: string): boolean;
	isKeyRepeat(data: string): boolean;
	Key: Record<string, unknown>;
};

type StdinBufferApi = {
	StdinBuffer: new (options?: { timeout?: number }) => {
		process(data: string | Buffer): void;
		flush(): string[];
		getBuffer(): string;
		destroy(): void;
		on(event: "data" | "paste", handler: (data: string) => void): void;
	};
};

type TerminalImageApi = {
	setCapabilities(caps: { images: "kitty" | "iterm2" | null; trueColor: boolean; hyperlinks: boolean }): void;
	resetCapabilitiesCache(): void;
	setCellDimensions(dims: { widthPx: number; heightPx: number }): void;
	detectCapabilities(tmuxForwardsHyperlink: () => boolean): {
		images: "kitty" | "iterm2" | null;
		trueColor: boolean;
		hyperlinks: boolean;
	};
	isImageLine(line: string): boolean;
	encodeKitty(
		data: string,
		options: { columns?: number; rows?: number; imageId?: number; moveCursor?: boolean },
	): string;
	encodeITerm2(
		data: string,
		options: {
			width?: number | string;
			height?: number | string;
			name?: string;
			preserveAspectRatio?: boolean;
			inline?: boolean;
		},
	): string;
	deleteKittyImage(imageId: number): string;
	deleteAllKittyImages(): string;
	calculateImageCellSize(
		image: { widthPx: number; heightPx: number },
		maxWidthCells: number,
		maxHeightCells: number | undefined,
		cell: { widthPx: number; heightPx: number },
	): { columns: number; rows: number };
	getPngDimensions(data: string): { widthPx: number; heightPx: number } | null;
	getJpegDimensions(data: string): { widthPx: number; heightPx: number } | null;
	getGifDimensions(data: string): { widthPx: number; heightPx: number } | null;
	getWebpDimensions(data: string): { widthPx: number; heightPx: number } | null;
	getImageDimensions(data: string, mimeType: string): { widthPx: number; heightPx: number } | null;
	renderImage(
		data: string,
		image: { widthPx: number; heightPx: number },
		options: {
			maxWidthCells?: number;
			maxHeightCells?: number;
			preserveAspectRatio?: boolean;
			imageId?: number;
			moveCursor?: boolean;
		},
	): { sequence: string; rows: number; imageId?: number } | null;
	hyperlink(text: string, url: string): string;
	imageFallback(
		mimeType: string,
		dimensions: { widthPx: number; heightPx: number } | undefined,
		filename: string | undefined,
	): string;
};

const TERMINAL_ENV_KEYS = [
	"TERM",
	"TERM_PROGRAM",
	"TERMINAL_EMULATOR",
	"COLORTERM",
	"TMUX",
	"KITTY_WINDOW_ID",
	"GHOSTTY_RESOURCES_DIR",
	"WEZTERM_PANE",
	"ITERM_SESSION_ID",
	"WT_SESSION",
	"CMUX_WORKSPACE_ID",
	"WARP_SESSION_ID",
	"WARP_TERMINAL_SESSION_UUID",
];

function withTerminalEnvironment<T>(values: Record<string, string | null>, action: () => T): T {
	const saved = new Map<string, string | undefined>();
	for (const key of TERMINAL_ENV_KEYS) {
		saved.set(key, process.env[key]);
		delete process.env[key];
	}
	try {
		for (const [key, value] of Object.entries(values)) {
			if (value !== null) process.env[key] = value;
		}
		return action();
	} finally {
		for (const [key, value] of saved) {
			if (value === undefined) delete process.env[key];
			else process.env[key] = value;
		}
	}
}

async function loadApi(sourceDirectory: string, moduleName: string): Promise<unknown> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
		case "fuzzy_match": {
			const api = (await loadApi(sourceDirectory, "fuzzy.ts")) as FuzzyMatchApi;
			const cases = requireArray<{ query: string; text: string }>(request, "cases");
			return {
				results: cases.map((testCase) => {
					const result = api.fuzzyMatch(testCase.query, testCase.text);
					return { matches: result.matches, scoreBits: f64Bits(result.score) };
				}),
			};
		}
		case "fuzzy_filter": {
			const api = (await loadApi(sourceDirectory, "fuzzy.ts")) as FuzzyMatchApi;
			const cases = requireArray<{ query: string; items: string[][] }>(request, "cases");
			return {
				results: cases.map((testCase) =>
					api.fuzzyFilter(testCase.items, testCase.query, (candidates) => candidates),
				),
			};
		}
		case "kill_ring": {
			const api = (await loadApi(sourceDirectory, "kill-ring.ts")) as KillRingApi;
			const cases = requireArray<KillRingOperation[]>(request, "cases");
			return { results: cases.map((operations) => simulateKillRing(api, operations)) };
		}
		case "terminal_colors": {
			const api = (await loadApi(sourceDirectory, "terminal-colors.ts")) as TerminalColorsApi;
			const cases = requireArray<string>(request, "cases");
			return {
				results: cases.map((data) => ({
					rgb: api.parseOsc11BackgroundColor(data) ?? null,
					isResponse: api.isOsc11BackgroundColorResponse(data),
					scheme: api.parseTerminalColorSchemeReport(data) ?? null,
				})),
			};
		}
		case "utils_visible_width": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<string>(request, "cases");
			return { results: cases.map((value) => api.visibleWidth(value)) };
		}
		case "utils_normalize": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<string>(request, "cases");
			return { results: cases.map((value) => api.normalizeTerminalOutput(value)) };
		}
		case "utils_extract_ansi": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<string>(request, "cases");
			return { results: cases.map((value) => scanAnsiCodes(api, value)) };
		}
		case "utils_truncate": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<{ text: string; maxWidth: number; ellipsis: string; pad: boolean }>(
				request,
				"cases",
			);
			return {
				results: cases.map((testCase) =>
					api.truncateToWidth(testCase.text, testCase.maxWidth, testCase.ellipsis, testCase.pad),
				),
			};
		}
		case "utils_wrap": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<{ text: string; width: number }>(request, "cases");
			return { results: cases.map((testCase) => api.wrapTextWithAnsi(testCase.text, testCase.width)) };
		}
		case "utils_slice": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<{ line: string; start: number; length: number; strict: boolean }>(
				request,
				"cases",
			);
			return {
				results: cases.map((testCase) =>
					api.sliceWithWidth(testCase.line, testCase.start, testCase.length, testCase.strict),
				),
			};
		}
		case "utils_extract_segments": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<{
				line: string;
				beforeEnd: number;
				afterStart: number;
				afterLen: number;
				strictAfter: boolean;
			}>(request, "cases");
			return {
				results: cases.map((testCase) =>
					api.extractSegments(
						testCase.line,
						testCase.beforeEnd,
						testCase.afterStart,
						testCase.afterLen,
						testCase.strictAfter,
					),
				),
			};
		}
		case "utils_background": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<{ line: string; width: number }>(request, "cases");
			return {
				results: cases.map((testCase) =>
					api.applyBackgroundToLine(testCase.line, testCase.width, (text) => `[${text}]`),
				),
			};
		}
		case "utils_char_classes": {
			const api = (await loadApi(sourceDirectory, "utils.ts")) as UtilsApi;
			const cases = requireArray<string>(request, "cases");
			return {
				results: cases.map((value) => ({
					whitespace: api.isWhitespaceChar(value),
					punctuation: api.isPunctuationChar(value),
				})),
			};
		}
		case "keys_parse": {
			const api = (await loadApi(sourceDirectory, "keys.ts")) as KeysApi;
			const cases = requireArray<{ data: string; kitty: boolean }>(request, "cases");
			return {
				results: cases.map((testCase) => {
					api.setKittyProtocolActive(testCase.kitty);
					try {
						return api.parseKey(testCase.data) ?? null;
					} finally {
						api.setKittyProtocolActive(false);
					}
				}),
			};
		}
		case "keys_match": {
			const api = (await loadApi(sourceDirectory, "keys.ts")) as KeysApi;
			const cases = requireArray<{ data: string; keyId: string; kitty: boolean }>(request, "cases");
			return {
				results: cases.map((testCase) => {
					api.setKittyProtocolActive(testCase.kitty);
					try {
						return api.matchesKey(testCase.data, testCase.keyId);
					} finally {
						api.setKittyProtocolActive(false);
					}
				}),
			};
		}
		case "keys_constants": {
			const api = (await loadApi(sourceDirectory, "keys.ts")) as KeysApi;
			const cases = requireArray<{ name: string; args?: string[] }>(request, "cases");
			const constants: Record<string, unknown> = {};
			const calls: Record<string, unknown> = {};
			for (const testCase of cases) {
				const value = api.Key[testCase.name];
				if (typeof value === "function") {
					calls[testCase.name] = (value as (...args: string[]) => string)(...(testCase.args ?? []));
				} else {
					constants[testCase.name] = value;
				}
			}
			return { constants, calls };
		}
		case "keys_decode": {
			const api = (await loadApi(sourceDirectory, "keys.ts")) as KeysApi;
			const cases = requireArray<string>(request, "cases");
			return {
				results: cases.map((data) => ({
					kitty: api.decodeKittyPrintable(data) ?? null,
					printable: api.decodePrintableKey(data) ?? null,
					release: api.isKeyRelease(data),
					repeat: api.isKeyRepeat(data),
				})),
			};
		}
		case "native_modifiers": {
			const api = await loadApi(sourceDirectory, "native-modifiers.ts") as { isNativeModifierPressed(key: string): boolean };
			return ["shift", "command", "control", "option"].map((key) => api.isNativeModifierPressed(key));
		}
		case "stdin_buffer": {
			const api = (await loadApi(sourceDirectory, "stdin-buffer.ts")) as StdinBufferApi;
			const cases = requireArray<{ chunks: string[]; timeout?: number }>(request, "cases");
			const results: Array<{ events: unknown[]; bufferAfterProcess: string }> = [];
			const buffers: Array<InstanceType<StdinBufferApi["StdinBuffer"]>> = [];
			for (const testCase of cases) {
				const events: unknown[] = [];
				const buffer = new api.StdinBuffer({ timeout: testCase.timeout });
				buffer.on("data", (value) => events.push({ kind: "data", value }));
				buffer.on("paste", (value) => events.push({ kind: "paste", value }));
				for (const chunk of testCase.chunks) buffer.process(chunk);
				buffers.push(buffer);
				results.push({ events, bufferAfterProcess: buffer.getBuffer() });
			}
			await new Promise((resolve) => setTimeout(resolve, 30));
			const output = results.map((result, index) => ({
				events: result.events,
				bufferAfterProcess: result.bufferAfterProcess,
				bufferAfterTimeout: buffers[index]?.getBuffer() ?? "",
			}));
			for (const buffer of buffers) buffer.destroy();
			return { results: output };
		}
		case "terminal_image_lines": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			const cases = requireArray<string>(request, "cases");
			return { results: cases.map((line) => api.isImageLine(line)) };
		}
		case "terminal_image_encode": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			const cases = requireArray<Record<string, unknown>>(request, "cases");
			return {
				results: cases.map((testCase) =>
					testCase.kind === "kitty"
						? api.encodeKitty(testCase.data as string, testCase.options as never)
						: api.encodeITerm2(testCase.data as string, testCase.options as never),
				),
			};
		}
		case "terminal_image_dimensions": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			const cases = requireArray<{ data: string; mimeType: string }>(request, "cases");
			return {
				results: cases.map((testCase) => ({
					png: api.getPngDimensions(testCase.data),
					jpeg: api.getJpegDimensions(testCase.data),
					gif: api.getGifDimensions(testCase.data),
					webp: api.getWebpDimensions(testCase.data),
					dispatched: api.getImageDimensions(testCase.data, testCase.mimeType),
				})),
			};
		}
		case "terminal_image_calc": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			const cases = requireArray<{
				image: { widthPx: number; heightPx: number };
				maxWidth: number;
				maxHeight?: number;
				cell: { widthPx: number; heightPx: number };
			}>(request, "cases");
			return {
				results: cases.map((testCase) =>
					api.calculateImageCellSize(testCase.image, testCase.maxWidth, testCase.maxHeight, testCase.cell),
				),
			};
		}
		case "terminal_image_detect": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			const cases = requireArray<{ env: Record<string, string | null>; tmux: boolean }>(request, "cases");
			return {
				results: cases.map((testCase) =>
					withTerminalEnvironment(testCase.env, () => api.detectCapabilities(() => testCase.tmux)),
				),
			};
		}
		case "terminal_image_render": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			const cases = requireArray<{
				caps: { images: "kitty" | "iterm2" | null; trueColor: boolean; hyperlinks: boolean };
				cell: { widthPx: number; heightPx: number };
				data: string;
				image: { widthPx: number; heightPx: number };
				options: Record<string, unknown>;
			}>(request, "cases");
			const results = cases.map((testCase) => {
				api.setCapabilities(testCase.caps);
				api.setCellDimensions(testCase.cell);
				try {
					const result = api.renderImage(testCase.data, testCase.image, testCase.options);
					return result === null ? null : { ...result, imageId: result.imageId ?? null };
				} finally {
					api.resetCapabilitiesCache();
					api.setCellDimensions({ widthPx: 9, heightPx: 18 });
				}
			});
			return { results };
		}
		case "terminal_image_fallback": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			const cases = requireArray<{
				caps: { images: "kitty" | "iterm2" | null; trueColor: boolean; hyperlinks: boolean };
				mimeType: string;
				dimensions: { widthPx: number; heightPx: number } | null;
				filename: string | null;
			}>(request, "cases");
			return {
				results: cases.map((testCase) => {
					api.setCapabilities(testCase.caps);
					try {
						return api.imageFallback(
							testCase.mimeType,
							testCase.dimensions ?? undefined,
							testCase.filename ?? undefined,
						);
					} finally {
						api.resetCapabilitiesCache();
					}
				}),
			};
		}
		case "terminal_image_hyperlink": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			const cases = requireArray<{ text: string; url: string }>(request, "cases");
			return { results: cases.map((testCase) => api.hyperlink(testCase.text, testCase.url)) };
		}
		case "terminal_image_delete": {
			const api = (await loadApi(sourceDirectory, "terminal-image.ts")) as TerminalImageApi;
			return {
				results: {
					one: api.deleteKittyImage(42),
					all: api.deleteAllKittyImages(),
				},
			};
		}
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
		throw new Error("tui leaves oracle requires a source directory path");
	}
	return sourceDirectory;
}

serveRequests(readSourceDirectory());
