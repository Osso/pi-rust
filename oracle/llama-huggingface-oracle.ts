import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { createInterface } from "node:readline";

type RequestId = string | number | null;

type HuggingFaceApi = {
	findHuggingFaceToken(env: Record<string, string | undefined>): Promise<string | undefined>;
	HuggingFaceClient: new (token?: string, baseUrl?: string) => {
		search(query: string): Promise<unknown>;
		details(id: string): Promise<unknown>;
	};
};

type ScriptedResponse = { status: number; headers?: Record<string, string>; body: unknown };

type Request =
	| { id: RequestId; op: "token"; env: Record<string, string | undefined> }
	| { id: RequestId; op: "search"; token?: string; response: ScriptedResponse }
	| { id: RequestId; op: "details"; token?: string; id: string; response: ScriptedResponse };

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

function send(response: ServerResponse, scripted: ScriptedResponse): void {
	response.writeHead(scripted.status, { "Content-Type": "application/json", ...scripted.headers });
	response.end(JSON.stringify(scripted.body));
}

async function withServer(
	scripted: ScriptedResponse,
	run: (url: string, requests: Array<{ url: string; authorization: string | null }>) => Promise<unknown>,
): Promise<unknown> {
	const requests: Array<{ url: string; authorization: string | null }> = [];
	const { server, url } = await listen((incoming, response) => {
		requests.push({ url: incoming.url ?? "", authorization: incoming.headers.authorization ?? null });
		send(response, scripted);
	});
	try {
		return await run(url, requests);
	} finally {
		server.close();
		server.closeAllConnections();
	}
}

async function execute(request: Request, api: HuggingFaceApi): Promise<unknown> {
	if (request.op === "token") return { token: (await api.findHuggingFaceToken(request.env)) ?? null };
	if (request.op === "search") {
		return withServer(request.response, async (url, requests) => {
			const client = new api.HuggingFaceClient(request.token, url);
			try {
				return { requests, result: await client.search("qwen coder") };
			} catch (error) {
				return { requests, error: errorMessage(error) };
			}
		});
	}
	return withServer(request.response, async (url, requests) => {
		const client = new api.HuggingFaceClient(request.token, url);
		try {
			return { requests, result: await client.details(request.id) };
		} catch (error) {
			return { requests, error: errorMessage(error) };
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("oracle requires the source module path");
	const api = (await import(modulePath)) as HuggingFaceApi;
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
