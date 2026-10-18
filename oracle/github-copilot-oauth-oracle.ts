import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type OAuthCredential = Record<string, unknown>;

type OAuthAuth = {
	refresh(credential: OAuthCredential, signal: AbortSignal): Promise<OAuthCredential>;
	toAuth(credential: OAuthCredential): Promise<Record<string, unknown>>;
};

type Request =
	| {
			id: RequestId;
			op: "refresh";
			cases: Array<{
				credential: OAuthCredential;
				token: Record<string, unknown>;
				models: Record<string, unknown>;
			}>;
	  }
	| { id: RequestId; op: "toAuth"; cases: Array<{ credential: OAuthCredential }> };

type CapturedRequest = {
	url: string;
	method: string;
	headers: Record<string, string>;
	body: string | null;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function execute(request: Request, api: OAuthAuth) {
	switch (request.op) {
		case "toAuth": {
			const results = [];
			for (const testCase of request.cases) {
				results.push({ auth: await api.toAuth(testCase.credential) });
			}
			return { results };
		}
		case "refresh": {
			const originalFetch = globalThis.fetch;
			const results = [];
			try {
				for (const testCase of request.cases) {
					const requests: CapturedRequest[] = [];
					globalThis.fetch = (async (input: unknown, init?: RequestInit) => {
						const headers: Record<string, string> = {};
						for (const [name, value] of new Headers(init?.headers).entries()) headers[name] = value;
						requests.push({
							url: String(input),
							method: init?.method ?? "GET",
							headers,
							body: typeof init?.body === "string" ? init.body : null,
						});
						const isModels = String(input).endsWith("/models");
						const body = isModels ? testCase.models : testCase.token;
						return new Response(JSON.stringify(body), {
							status: 200,
							headers: { "content-type": "application/json" },
						});
					}) as typeof fetch;
					try {
						const credential = await api.refresh(testCase.credential, new AbortController().signal);
						results.push({ credential, requests });
					} catch (error) {
						results.push({ error: normalizeError(error), requests });
					}
				}
			} finally {
				globalThis.fetch = originalFetch;
			}
			return { results };
		}
	}
}

function executeRequestLine(line: string, api: OAuthAuth): void {
	let id: RequestId = null;
	let pending: Promise<unknown>;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		pending = execute(request, api);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
		return;
	}
	void pending
		.then((output) => process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`))
		.catch((error: unknown) =>
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`),
		);
}

function serveRequests(api: OAuthAuth): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("github copilot oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as { githubCopilotOAuth: OAuthAuth };
	serveRequests(api.githubCopilotOAuth);
}

void main().catch((error: unknown) => {
	process.stderr.write(`github copilot oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
