/**
 * JSON-lines oracle for the coding-agent image pipeline.
 *
 * The oracle receives the path to one pinned utils module and imports its
 * siblings, so every operation calls the real TypeScript implementation.
 */
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Request = { id: RequestId; operation: string; [key: string]: unknown };

type Case = { name: string; [key: string]: unknown };

type PhotonImageHandle = {
	get_width(): number;
	get_height(): number;
	get_raw_pixels(): Uint8Array;
	free(): void;
};

type ResizedImageRecord = {
	data: string;
	mimeType: string;
	originalWidth: number;
	originalHeight: number;
	width: number;
	height: number;
	wasResized: boolean;
};

type Modules = {
	photon: {
		PhotonImage: { new_from_byteslice(bytes: Uint8Array): PhotonImageHandle };
	};
	exif: {
		applyExifOrientation(photonModule: unknown, image: PhotonImageHandle, bytes: Uint8Array): PhotonImageHandle;
	};
	convert: {
		convertImageBytesToPng(bytes: Uint8Array): Promise<Uint8Array | null>;
		convertToPng(data: string, mimeType: string): Promise<{ data: string; mimeType: string } | null>;
	};
	core: {
		resizeImageInProcess(bytes: Uint8Array, mimeType: string, options?: unknown): Promise<ResizedImageRecord | null>;
	};
	resize: {
		resizeImage(bytes: Uint8Array, mimeType: string, options?: unknown): Promise<ResizedImageRecord | null>;
		formatDimensionNote(result: ResizedImageRecord): string | undefined;
	};
	imageProcess: {
		processImage(bytes: Uint8Array, mimeType: string, options?: unknown): Promise<unknown>;
	};
	toolResultImages: {
		normalizeToolResultImages(content: unknown[], options?: unknown): Promise<unknown[]>;
	};
};

async function importSibling(directory: string, name: string): Promise<Record<string, unknown>> {
	return (await import(pathToFileURL(join(directory, name)).href)) as Record<string, unknown>;
}

async function loadModules(sourcePath: string): Promise<Modules> {
	const directory = dirname(sourcePath);
	// `photon.ts` only exposes the loader; the oracle needs the WASM module it loads.
	const photon = createRequire(sourcePath)("@silvia-odwyer/photon-node") as Modules["photon"];
	return {
		photon,
		exif: (await importSibling(directory, "exif-orientation.ts")) as unknown as Modules["exif"],
		convert: (await importSibling(directory, "image-convert.ts")) as unknown as Modules["convert"],
		core: (await importSibling(directory, "image-resize-core.ts")) as unknown as Modules["core"],
		resize: (await importSibling(directory, "image-resize.ts")) as unknown as Modules["resize"],
		imageProcess: (await importSibling(directory, "image-process.ts")) as unknown as Modules["imageProcess"],
		toolResultImages: (await importSibling(directory, "tool-result-images.ts")) as unknown as Modules["toolResultImages"],
	};
}

function bytesOf(value: unknown): Uint8Array {
	return Buffer.from(String(value ?? ""), "hex");
}

function hexOf(bytes: Uint8Array): string {
	return Buffer.from(bytes).toString("hex");
}

function executeExifOrientation(modules: Modules, cases: Case[]): unknown {
	const results = cases.map((imageCase) => {
		const bytes = bytesOf(imageCase.bytesHex);
		const image = modules.photon.PhotonImage.new_from_byteslice(bytes);
		const applied = modules.exif.applyExifOrientation(modules.photon, image, bytes);
		try {
			return {
				name: imageCase.name,
				width: applied.get_width(),
				height: applied.get_height(),
				pixelsHex: hexOf(applied.get_raw_pixels()),
			};
		} finally {
			if (applied !== image) applied.free();
			image.free();
		}
	});
	return { results };
}

async function executeConvert(modules: Modules, cases: Case[]): Promise<unknown> {
	const results = await Promise.all(
		cases.map(async (imageCase) => {
			const converted = await modules.convert.convertToPng(String(imageCase.base64), String(imageCase.mimeType));
			return converted === null
				? { name: imageCase.name, data: null, mimeType: null }
				: { name: imageCase.name, data: converted.data, mimeType: converted.mimeType };
		}),
	);
	return { results };
}

async function executeConvertBytes(modules: Modules, cases: Case[]): Promise<unknown> {
	const results = await Promise.all(
		cases.map(async (imageCase) => {
			const converted = await modules.convert.convertImageBytesToPng(bytesOf(imageCase.bytesHex));
			return { name: imageCase.name, data: converted === null ? null : Buffer.from(converted).toString("base64") };
		}),
	);
	return { results };
}

async function executeResize(modules: Modules, cases: Case[], useWorker: boolean): Promise<unknown> {
	const results = [];
	for (const imageCase of cases) {
		const bytes = bytesOf(imageCase.bytesHex);
		const options = imageCase.options ?? undefined;
		const result = useWorker
			? await modules.resize.resizeImage(bytes, String(imageCase.mimeType), options)
			: await modules.core.resizeImageInProcess(bytes, String(imageCase.mimeType), options);
		results.push({ name: imageCase.name, result });
	}
	return { results };
}

function executeDimensionNotes(modules: Modules, cases: Case[]): unknown {
	const results = cases.map((noteCase) => {
		const value = modules.resize.formatDimensionNote(noteCase.result as ResizedImageRecord);
		return { name: noteCase.name, value: value ?? null };
	});
	return { results };
}

async function executeProcessImage(modules: Modules, cases: Case[]): Promise<unknown> {
	const results = [];
	for (const imageCase of cases) {
		const options = imageCase.options ?? undefined;
		const result = await modules.imageProcess.processImage(
			bytesOf(imageCase.bytesHex),
			String(imageCase.mimeType),
			options,
		);
		results.push({ name: imageCase.name, result });
	}
	return { results };
}

async function executeNormalizeToolResultImages(modules: Modules, cases: Case[]): Promise<unknown> {
	const results = [];
	for (const imageCase of cases) {
		const content = imageCase.content as unknown[];
		const options = imageCase.options ?? undefined;
		const normalized = await modules.toolResultImages.normalizeToolResultImages(content, options);
		results.push({ name: imageCase.name, changed: normalized !== content, content: normalized });
	}
	return { results };
}

async function execute(modules: Modules, request: Request): Promise<unknown> {
	const cases = (request.cases ?? []) as Case[];
	switch (request.operation) {
		case "exif_orientation":
			return executeExifOrientation(modules, cases);
		case "convert_to_png":
			return executeConvert(modules, cases);
		case "convert_bytes_to_png":
			return executeConvertBytes(modules, cases);
		case "resize_in_process":
			return executeResize(modules, cases, false);
		case "resize_in_worker":
			return executeResize(modules, cases, true);
		case "format_dimension_note":
			return executeDimensionNotes(modules, cases);
		case "process_image":
			return executeProcessImage(modules, cases);
		case "normalize_tool_result_images":
			return executeNormalizeToolResultImages(modules, cases);
		default:
			throw new Error(`unknown image pipeline operation: ${request.operation}`);
	}
}

async function executeRequestLine(modules: Modules, line: string): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const output = await execute(modules, request);
		process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
	} catch (error) {
		const normalized =
			error instanceof Error
				? { name: error.name, message: error.message }
				: { name: "UnknownError", message: String(error) };
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalized })}\n`);
	}
}

function serveRequests(modules: Modules): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let queue: Promise<void> = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(async () => executeRequestLine(modules, line));
		}
	});
}

async function main(): Promise<void> {
	const sourcePath = process.argv[2];
	if (sourcePath === undefined) {
		throw new Error("image pipeline oracle requires a source module path");
	}
	serveRequests(await loadModules(sourcePath));
}

void main().catch((error: unknown) => {
	process.stderr.write(`image pipeline oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
