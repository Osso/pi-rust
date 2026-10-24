import { dirname, join } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import {
	clearFetchScript,
	installScriptedFetch,
	setFetchScript,
} from "./scripted-fetch.ts";

type RequestId = string | number | null;

type Model = Record<string, unknown>;

type Provider = {
	id: string;
	name: string;
	baseUrl?: string;
	headers?: Record<string, string | null>;
	auth: {
		apiKey?: {
			name: string;
			login?: (interaction: unknown) => Promise<unknown>;
			resolve(input: unknown): Promise<unknown>;
		};
		oauth?: { name: string; isSubscription?: boolean; loginLabel?: string };
	};
	getModels(): Model[];
	filterModels?: (models: Model[], credential: unknown) => Model[];
	streamSimple(
		model: Model,
		context: unknown,
		options?: unknown,
	): { result(): Promise<unknown> };
};

type Models = {
	setProvider(provider: Provider): void;
	getAuth(model: Model): Promise<unknown>;
};

type Api = {
	builtinProviders(): Provider[];
	getBuiltinProviders(): string[];
	getBuiltinModels(provider: string): Model[];
	getBuiltinModel(provider: string, modelId: string): Model | undefined;
};

type Request =
	| { id: RequestId; op: "providers" }
	| { id: RequestId; op: "catalog" }
	| {
			id: RequestId;
			op: "auth";
			cases: Array<{
				provider: string;
				model: Model;
				files?: string[];
				env: Record<string, string>;
				credential?: unknown;
			}>;
	  }
	| {
			id: RequestId;
			op: "login";
			cases: Array<{ provider: string; answers: string[] }>;
	  }
	| {
			id: RequestId;
			op: "cloudflare";
			cases: Array<{
				model: Model;
				env?: Record<string, string>;
				credential?: unknown;
			}>;
	  }
	| {
			id: RequestId;
			op: "dispatch";
			cases: Array<{ provider: string; modelId: string; api?: string }>;
	  };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function fakeAuthContext(env: Record<string, string>, files: string[] = []) {
	return {
		env: async (name: string) => env[name],
		fileExists: async (path: string) => files.includes(path),
	};
}

const neverAbortedSignal = new AbortController().signal;

function describeProvider(provider: Provider) {
	return {
		id: provider.id,
		name: provider.name,
		baseUrl: provider.baseUrl ?? null,
		headers: provider.headers ?? null,
		apiKeyName: provider.auth.apiKey?.name ?? null,
		oauthName: provider.auth.oauth?.name ?? null,
		oauthSubscription: provider.auth.oauth
			? Boolean(provider.auth.oauth.isSubscription)
			: null,
		oauthLoginLabel: provider.auth.oauth?.loginLabel ?? null,
		models: provider.getModels().map((model) => model.id),
		pickerModels: (
			provider.filterModels?.(provider.getModels(), {
				type: "oauth",
				access: "access",
				refresh: "refresh",
				expires: 1e15,
				availableModelIds: [],
			}) ?? provider.getModels()
		).map((model) => model.id),
	};
}

async function executeAuth(
	api: Api,
	modelsModule: { createModels(options?: unknown): Models },
	request: Extract<Request, { op: "auth" }>,
) {
	const results = [];
	for (const testCase of request.cases) {
		const provider = api
			.builtinProviders()
			.find((candidate) => candidate.id === testCase.provider);
		if (!provider) throw new Error(`Unknown provider ${testCase.provider}`);
		const models = modelsModule.createModels({
			authContext: fakeAuthContext(testCase.env, testCase.files ?? []),
		});
		models.setProvider(provider);
		const result = await models.getAuth(testCase.model);
		results.push({ provider: testCase.provider, result: result ?? null });
	}
	return { results };
}

async function executeLogin(
	api: Api,
	request: Extract<Request, { op: "login" }>,
) {
	const results = [];
	for (const testCase of request.cases) {
		const provider = api
			.builtinProviders()
			.find((candidate) => candidate.id === testCase.provider);
		if (!provider) throw new Error(`Unknown provider ${testCase.provider}`);
		const answers = [...testCase.answers];
		const events: unknown[] = [];
		const prompts: unknown[] = [];
		try {
			const credential = await provider.auth.apiKey?.login?.({
				signal: neverAbortedSignal,
				prompt: async (prompt: {
					type: string;
					message: string;
					options?: unknown[];
				}) => {
					prompts.push(prompt);
					return answers.shift() ?? "";
				},
				notify: (event: unknown) => events.push(event),
			});
			results.push({
				provider: testCase.provider,
				credential: credential ?? null,
				events,
				prompts,
			});
		} catch (error) {
			results.push({
				provider: testCase.provider,
				error: normalizeError(error),
				events,
				prompts,
			});
		}
	}
	return { results };
}

async function executeDispatch(
	api: Api,
	request: Extract<Request, { op: "dispatch" }>,
) {
	const results = [];
	try {
		for (const testCase of request.cases) {
			const provider = api
				.builtinProviders()
				.find((candidate) => candidate.id === testCase.provider);
			if (!provider) throw new Error(`Unknown provider ${testCase.provider}`);
			const catalogModel = provider
				.getModels()
				.find((candidate) => candidate.id === testCase.modelId);
			if (!catalogModel)
				throw new Error(
					`Unknown model ${testCase.provider}/${testCase.modelId}`,
				);
			const model =
				testCase.api === undefined
					? catalogModel
					: { ...catalogModel, api: testCase.api };
			const requests: unknown[] = [];
			setFetchScript(async (input, init) => {
				const request = new Request(input, init);
				requests.push({
					url: request.url,
					method: request.method,
					headers: Object.fromEntries(request.headers),
					body: request.body === null ? null : await request.clone().text(),
				});
				return new Response(
					JSON.stringify({ error: { message: "recorded" } }),
					{
						status: 400,
						headers: { "content-type": "application/json" },
					},
				);
			});
			try {
				const result = (await provider
					.streamSimple(model, { messages: [] }, { apiKey: "test-key" })
					.result()) as { stopReason?: string; errorMessage?: string };
				results.push({
					provider: testCase.provider,
					modelId: testCase.modelId,
					requests,
					stopReason: result.stopReason ?? null,
					errorMessage: result.errorMessage ?? null,
				});
			} catch (error) {
				results.push({
					provider: testCase.provider,
					modelId: testCase.modelId,
					requests,
					error: normalizeError(error),
				});
			}
		}
	} finally {
		clearFetchScript();
	}
	return { results };
}

async function execute(request: Request, api: Api, srcDir: string) {
	switch (request.op) {
		case "providers":
			return { providers: api.builtinProviders().map(describeProvider) };
		case "catalog":
			return {
				providers: api.getBuiltinProviders(),
				models: Object.fromEntries(
					api
						.getBuiltinProviders()
						.map((provider) => [provider, api.getBuiltinModels(provider)]),
				),
			};
		case "auth": {
			const modelsModule = (await import(
				pathToFileURL(join(srcDir, "models.ts")).href
			)) as {
				createModels(options?: unknown): Models;
			};
			return executeAuth(api, modelsModule, request);
		}
		case "login":
			return executeLogin(api, request);
		case "cloudflare":
			return {
				results: await Promise.all(
					request.cases.map(async (testCase) => {
						const provider = api
							.builtinProviders()
							.find((candidate) => candidate.id === testCase.model.provider);
						if (!provider?.auth.apiKey)
							throw new Error(
								`Missing Cloudflare auth ${testCase.model.provider}`,
							);
						return (
							(await provider.auth.apiKey.resolve({
								model: testCase.model,
								ctx: fakeAuthContext(testCase.env ?? {}),
								credential: testCase.credential,
								signal: neverAbortedSignal,
							})) ?? null
						);
					}),
				),
			};
		case "dispatch":
			return executeDispatch(api, request);
	}
}

function executeRequestLine(line: string, api: Api, srcDir: string): void {
	let id: RequestId = null;
	let pending: Promise<unknown>;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		pending = execute(request, api, srcDir);
	} catch (error) {
		process.stdout.write(
			`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`,
		);
		return;
	}
	void pending
		.then((output) => {
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		})
		.catch((error: unknown) => {
			process.stdout.write(
				`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`,
			);
		});
}

function serveRequests(api: Api, srcDir: string): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api, srcDir);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("providers oracle requires a source module path");
	installScriptedFetch();
	const api = (await import(pathToFileURL(modulePath).href)) as Api;
	serveRequests(api, dirname(dirname(modulePath)));
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`providers oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
