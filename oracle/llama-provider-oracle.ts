import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { createInterface } from "node:readline";

type RequestId = string | number | null;

type ProviderApi = {
	LLAMA_PROVIDER_ID: string;
	DEFAULT_LLAMA_SERVER_URL: string;
	createLlamaProvider(): {
		provider: {
			id: string;
			name: string;
			baseUrl?: string;
			auth: {
				apiKey?: {
					name: string;
					login(interaction: unknown): Promise<unknown>;
					check(input: unknown): Promise<unknown>;
					resolve(input: unknown): Promise<unknown>;
				};
			};
			getModels(): unknown[];
			refreshModels?(context: unknown): Promise<void>;
		};
		setCatalog(models: unknown[], serverUrl: string): void;
	};
};

type AuthCase = {
	env: Record<string, string>;
	credential?: { type: "api_key"; key?: string; env?: Record<string, string> };
};

type Request =
	| { id: RequestId; op: "catalog"; serverUrl: string; models: unknown[] }
	| { id: RequestId; op: "auth"; cases: AuthCase[] }
	| { id: RequestId; op: "login"; answers: string[]; catalog: unknown }
	| { id: RequestId; op: "refresh"; credentialEnv: string; stored: unknown; allowNetwork: boolean; publishResult: boolean; catalog: unknown };

function errorMessage(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

function listen(handler: (request: IncomingMessage, response: ServerResponse) => void): Promise<{ server: ReturnType<typeof createServer>; url: string }> {
	return new Promise((resolve) => {
		const server = createServer(handler);
		server.listen(0, "127.0.0.1", () => {
			const address = server.address() as AddressInfo;
			resolve({ server, url: `http://127.0.0.1:${address.port}` });
		});
	});
}

function json(response: ServerResponse, value: unknown): void {
	response.writeHead(200, { "Content-Type": "application/json" });
	response.end(JSON.stringify(value));
}

function normalizeModel(value: unknown): unknown {
	return value === undefined ? null : value;
}

function normalizePublication(publication: { persist?: unknown; update?: () => void }): unknown {
	if (publication.persist === undefined) return null;
	if (publication.persist === null) return null;
	const persist = publication.persist as { models?: unknown; checkedAt?: number };
	return {
		models: persist.models,
		checkedAt: persist.checkedAt === undefined ? null : 0,
	};
}

async function executeCatalog(api: ProviderApi, request: Extract<Request, { op: "catalog" }>): Promise<unknown> {
	const controller = api.createLlamaProvider();
	controller.setCatalog(request.models, request.serverUrl);
	return { models: controller.provider.getModels() };
}

async function executeAuth(api: ProviderApi, request: Extract<Request, { op: "auth" }>): Promise<unknown> {
	const controller = api.createLlamaProvider();
	const auth = controller.provider.auth.apiKey!;
	const results = [];
	for (const entry of request.cases) {
		const ctx = {
			env: async (name: string) => entry.env[name],
			fileExists: async () => false,
		};
		const signal = new AbortController().signal;
		const credential = entry.credential;
		results.push({
			check: normalizeModel(await auth.check({ ctx, credential, signal })),
			resolve: normalizeModel(await auth.resolve({ ctx, credential, signal })),
		});
	}
	return { name: auth.name, results };
}

async function executeLogin(api: ProviderApi, request: Extract<Request, { op: "login" }>): Promise<unknown> {
	const requests: Array<{ url: string; authorization: string | null }> = [];
	const { server, url } = await listen((incoming, response) => {
		requests.push({ url: incoming.url ?? "", authorization: incoming.headers.authorization ?? null });
		json(response, request.catalog);
	});
	try {
		const controller = api.createLlamaProvider();
		const auth = controller.provider.auth.apiKey!;
		const prompts: unknown[] = [];
		const answers = [...request.answers];
		const interaction = {
			signal: new AbortController().signal,
			prompt: async (prompt: unknown) => {
				prompts.push(prompt);
				const answer = answers.shift() ?? "";
				return answer === "\0url" ? url : answer;
			},
			notify: () => {},
		};
		const credential = await auth.login(interaction);
		return { credential, prompts, requests, baseUrl: url };
	} finally {
		server.close();
		server.closeAllConnections();
	}
}

async function executeRefresh(api: ProviderApi, request: Extract<Request, { op: "refresh" }>): Promise<unknown> {
	const requests: Array<{ url: string; authorization: string | null }> = [];
	const { server, url } = await listen((incoming, response) => {
		requests.push({ url: incoming.url ?? "", authorization: incoming.headers.authorization ?? null });
		json(response, request.catalog);
	});
	try {
		const controller = api.createLlamaProvider();
		const publications: unknown[] = [];
		const publish = async (publication: { persist?: unknown; update?: () => void }) => {
			publications.push(normalizePublication(publication));
			publication.update?.();
			return request.publishResult;
		};
		const credential = {
			type: "api_key" as const,
			key: "local",
			env: { LLAMA_BASE_URL: url },
		};
		await controller.provider.refreshModels?.({
			credential,
			stored: request.stored,
			publish,
			allowNetwork: request.allowNetwork,
			signal: new AbortController().signal,
		});
		return { models: controller.provider.getModels(), publications, requests, baseUrl: url };
	} finally {
		server.close();
		server.closeAllConnections();
	}
}

async function execute(request: Request, api: ProviderApi): Promise<unknown> {
	if (request.op === "catalog") return executeCatalog(api, request);
	if (request.op === "auth") return executeAuth(api, request);
	if (request.op === "login") return executeLogin(api, request);
	return executeRefresh(api, request);
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("oracle requires the source module path");
	const api = (await import(modulePath)) as ProviderApi;
	const lines = createInterface({ input: process.stdin });
	for await (const line of lines) {
		if (!line.trim()) continue;
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const output = await execute(request, api);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: errorMessage(error) })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`${String(error)}\n`);
	process.exitCode = 1;
});
