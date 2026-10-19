import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { createInterface } from "node:readline";

type RequestId = string | number | null;

type LlamaClientApi = {
	formatBytes(bytes: number): string;
	normalizeLlamaServerUrl(value: string): string;
	llamaInferenceUrl(value: string): string;
	LlamaClient: new (serverUrl: string, apiKey?: string) => {
		serverUrl: string;
		list(options?: { reload?: boolean }): Promise<unknown>;
		load(model: string): Promise<void>;
		unload(model: string): Promise<void>;
		download(model: string): Promise<void>;
	};
};

type LeafCase =
	| { name: string; kind: "format_bytes"; input: number }
	| { name: string; kind: "normalize" | "inference"; input: string };

type RecordedRequest = {
	method: string;
	url: string;
	contentType: string | null;
	authorization: string | null;
	body: string;
};

type RequestsOp = {
	op: "requests";
	apiKey?: string;
	actions: Array<"list" | "list_reload" | "load" | "unload" | "download">;
};

type CatalogErrorsOp = {
	op: "catalog_errors";
	responses: Array<{ status: number; body: unknown }>;
};

type LeavesOp = { op: "leaves"; cases: LeafCase[] };

type Request = { id: RequestId } & (LeavesOp | RequestsOp | CatalogErrorsOp);

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

function json(response: ServerResponse, value: unknown, status = 200): void {
	response.writeHead(status, { "Content-Type": "application/json" });
	response.end(JSON.stringify(value));
}

function readBody(request: IncomingMessage): Promise<string> {
	return new Promise((resolve) => {
		let body = "";
		request.on("data", (chunk: Buffer) => {
			body += chunk.toString("utf8");
		});
		request.on("end", () => resolve(body));
	});
}

function record(request: IncomingMessage, body: string): RecordedRequest {
	return {
		method: request.method ?? "GET",
		url: request.url ?? "",
		contentType: request.headers["content-type"] ?? null,
		authorization: request.headers.authorization ?? null,
		body: body === "" ? null : body,
	};
}

async function executeLeaves(api: LlamaClientApi, request: LeavesOp): Promise<unknown> {
	return {
		results: request.cases.map((entry) => {
			try {
				if (entry.kind === "format_bytes") return { name: entry.name, value: api.formatBytes(entry.input) };
				if (entry.kind === "normalize") return { name: entry.name, value: api.normalizeLlamaServerUrl(entry.input) };
				return { name: entry.name, value: api.llamaInferenceUrl(entry.input) };
			} catch (error) {
				return { name: entry.name, error: errorMessage(error) };
			}
		}),
	};
}

async function executeRequests(api: LlamaClientApi, request: RequestsOp): Promise<unknown> {
	const requests: RecordedRequest[] = [];
	const { server, url } = await listen((incoming, response) => {
		void readBody(incoming).then((body) => {
			requests.push(record(incoming, body));
			if (incoming.method === "GET" && (incoming.url === "/models?reload=1" || incoming.url === "/models")) {
				json(response, { data: [{ id: "loaded", status: { value: "loaded" } }] });
				return;
			}
			if (incoming.url === "/models/load" || incoming.url === "/models/unload") {
				json(response, { success: true });
				return;
			}
			if (incoming.url === "/models" && incoming.method === "POST") {
				json(response, { success: true });
				return;
			}
			json(response, {}, 404);
		});
	});
	try {
		const client = new api.LlamaClient(url, request.apiKey);
		const results: unknown[] = [];
		for (const action of request.actions) {
			try {
				if (action === "list") results.push({ action, value: await client.list() });
				else if (action === "list_reload") results.push({ action, value: await client.list({ reload: true }) });
				else if (action === "load") results.push({ action, value: await client.load("model") });
				else if (action === "unload") results.push({ action, value: await client.unload("model") });
				else results.push({ action, value: await client.download("owner/repo:Q4_K_M") });
			} catch (error) {
				results.push({ action, error: errorMessage(error) });
			}
		}
		return { requests, results, serverUrl: client.serverUrl };
	} finally {
		server.close();
		server.closeAllConnections();
	}
}

async function executeCatalogErrors(api: LlamaClientApi, request: CatalogErrorsOp): Promise<unknown> {
	let index = 0;
	const { server, url } = await listen((_incoming, response) => {
		const entry = request.responses[Math.min(index, request.responses.length - 1)]!;
		index += 1;
		json(response, entry.body, entry.status);
	});
	try {
		const client = new api.LlamaClient(url);
		const results: unknown[] = [];
		for (let attempt = 0; attempt < request.responses.length; attempt++) {
			try {
				results.push({ ok: true, value: await client.list() });
			} catch (error) {
				results.push({ ok: false, error: errorMessage(error) });
			}
		}
		return { results };
	} finally {
		server.close();
		server.closeAllConnections();
	}
}

async function execute(request: Request, api: LlamaClientApi): Promise<unknown> {
	if (request.op === "leaves") return executeLeaves(api, request);
	if (request.op === "requests") return executeRequests(api, request);
	return executeCatalogErrors(api, request);
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("oracle requires the source module path");
	const api = (await import(modulePath)) as LlamaClientApi;
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
