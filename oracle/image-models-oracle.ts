import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = {
	id: number;
	operation: "catalog" | "providers" | "models" | "model";
	provider?: string;
	modelId?: string;
};

type ImageModelsApi = {
	getImageProviders(): string[];
	getImageModels(provider: string): unknown[];
	getImageModel(provider: string, modelId: string): unknown;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: ImageModelsApi, catalog: unknown) {
	try {
		switch (request.operation) {
			case "catalog":
				return { id: request.id, ok: true, output: { catalog } };
			case "providers":
				return {
					id: request.id,
					ok: true,
					output: { providers: api.getImageProviders() },
				};
			case "models":
				return {
					id: request.id,
					ok: true,
					output: { models: api.getImageModels(request.provider ?? "") },
				};
			case "model":
				return {
					id: request.id,
					ok: true,
					output: {
						model:
							api.getImageModel(
								request.provider ?? "",
								request.modelId ?? "",
							) ?? null,
					},
				};
		}
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: ImageModelsApi, catalog: unknown): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			process.stdout.write(
				`${JSON.stringify(execute(JSON.parse(buffer.slice(0, lineFeed)), api, catalog))}\n`,
			);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("image models oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as ImageModelsApi;
	const { IMAGE_MODELS } = await import(
		new URL("./image-models.generated.ts", pathToFileURL(modulePath)).href
	);
	serve(api, IMAGE_MODELS);
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`image models oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
