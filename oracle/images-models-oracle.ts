import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = {
	id: number;
	reject?: boolean;
	operation: "read" | "generate" | "unknown";
	auth?: {
		apiKey?: string;
		headers?: Record<string, string | null>;
		baseUrl?: string;
		env?: Record<string, string>;
	} | null;
	options?: {
		metadata?: Record<string, unknown>;
		apiKey?: string;
		headers?: Record<string, string | null>;
		env?: Record<string, string>;
	};
	env?: Record<string, string>;
};

type AuthResultShape = {
	auth: {
		apiKey?: string;
		headers?: Record<string, string | null>;
		baseUrl?: string;
	};
	env?: Record<string, string>;
	source?: string;
};

type ImagesModelsApi = {
	createImagesModels(options: {
		authContext: {
			env(name: string): Promise<string | undefined>;
			fileExists(path: string): Promise<boolean>;
		};
	}): {
		setProvider(provider: unknown): void;
		getProviders(): Array<{ id: string }>;
		getModels(provider?: string): Array<{ id: string }>;
		getModel(provider: string, id: string): { id: string } | undefined;
		generateImages(
			model: unknown,
			context: unknown,
			options?: Record<string, unknown>,
		): Promise<Record<string, unknown>>;
	};
	createImagesProvider(input: {
		id: string;
		auth: unknown;
		models: unknown[];
		api: {
			generateImages(
				model: unknown,
				context: unknown,
				options?: Record<string, unknown>,
			): Promise<unknown>;
		};
	}): unknown;
};

function testModel(provider: string, id: string) {
	return {
		id,
		name: id,
		api: "test-images",
		provider,
		baseUrl: "https://example.test/v1",
		input: ["text"],
		output: ["image"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
	};
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function execute(request: Request, api: ImagesModelsApi) {
	const calls: Array<Record<string, unknown>> = [];
	const auth = request.auth === undefined ? null : request.auth;
	const authContext = {
		env: async (name: string) => request.env?.[name],
		fileExists: async () => false,
	};
	const models = api.createImagesModels({ authContext });
	models.setProvider(
		api.createImagesProvider({
			id: "p1",
			auth: {
				apiKey: {
					name: "Test key",
					resolve: async () =>
						auth
							? ({
									auth: {
										...(auth.apiKey === undefined
											? {}
											: { apiKey: auth.apiKey }),
										...(auth.headers === undefined
											? {}
											: { headers: auth.headers }),
										...(auth.baseUrl === undefined
											? {}
											: { baseUrl: auth.baseUrl }),
									},
									...(auth.env === undefined ? {} : { env: auth.env }),
								} satisfies AuthResultShape)
							: undefined,
				},
			},
			models: [testModel("p1", "m1"), testModel("p1", "m2")],
			api: {
				generateImages: async (model, _context, options) => {
					if (request.reject) throw new TypeError("image request failed");
					const typedModel = model as { id: string; baseUrl: string };
					calls.push({
						modelId: typedModel.id,
						baseUrl: typedModel.baseUrl,
						apiKey:
							(options as { apiKey?: string } | undefined)?.apiKey ?? null,
						headers:
							(options as { headers?: unknown } | undefined)?.headers ?? null,
						env: (options as { env?: unknown } | undefined)?.env ?? null,
						metadata: options?.metadata ?? null,
					});
					return {
						api: "test-images",
						provider: "p1",
						model: typedModel.id,
						output: [{ type: "image", data: "aGk=", mimeType: "image/png" }],
						stopReason: "stop",
						timestamp: 0,
					};
				},
			},
		}),
	);

	try {
		if (request.operation === "read") {
			return {
				id: request.id,
				ok: true,
				output: {
					providers: models.getProviders().map((provider) => provider.id),
					modelsAll: models.getModels().map((model) => model.id),
					modelsP1: models.getModels("p1").map((model) => model.id),
					modelFound: models.getModel("p1", "m1")?.id ?? null,
					modelMissing: models.getModel("p1", "missing") ?? null,
					modelsMissing: models.getModels("missing").map((model) => model.id),
				},
			};
		}

		if (request.operation === "unknown") {
			const result = await models.generateImages(testModel("ghost", "m"), {
				input: [],
			});
			return {
				id: request.id,
				ok: true,
				output: {
					stopReason: result.stopReason,
					errorMessage: result.errorMessage ?? null,
				},
			};
		}

		const result = await models.generateImages(
			testModel("p1", "m1"),
			{ input: [{ type: "text", text: "a red circle" }] },
			request.options ?? {},
		);
		return {
			id: request.id,
			ok: true,
			output: { result: { ...result, timestamp: 0 }, calls },
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: ImagesModelsApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const request = JSON.parse(buffer.slice(0, lineFeed)) as Request;
			buffer = buffer.slice(lineFeed + 1);
			void execute(request, api).then((response) => {
				process.stdout.write(`${JSON.stringify(response)}\n`);
			});
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("images models oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as ImagesModelsApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`images models oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
