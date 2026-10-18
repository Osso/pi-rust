import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Request =
	| {
			id: RequestId;
			operation: "oauth_html";
			cases: Array<{ name: string; kind: "success" | "error"; message: string; details?: string }>;
	  }
	| { id: RequestId; operation: "pkce"; count: number };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: Record<string, unknown>): Response {
	if (request.operation === "oauth_html") {
		const oauthSuccessHtml = api.oauthSuccessHtml as (message: string) => string;
		const oauthErrorHtml = api.oauthErrorHtml as (message: string, details?: string) => string;
		return {
			id: request.id,
			ok: true,
			output: {
				results: request.cases.map((item) => ({
					name: item.name,
					html: item.kind === "success" ? oauthSuccessHtml(item.message) : oauthErrorHtml(item.message, item.details),
				})),
			},
		};
	}
	throw new Error(`unsupported operation: ${(request as { operation: string }).operation}`);
}

async function executeAsync(request: Request, api: Record<string, unknown>): Promise<Response> {
	if (request.operation !== "pkce") return execute(request, api);
	const generatePKCE = api.generatePKCE as () => Promise<{ verifier: string; challenge: string }>;
	const results = [];
	for (let index = 0; index < request.count; index++) results.push(await generatePKCE());
	return { id: request.id, ok: true, output: { results } };
}

async function executeRequestLine(line: string, api: Record<string, unknown>): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(await executeAsync(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: Record<string, unknown>): void {
	let buffer = "";
	let queue = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += chunk.toString();
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(() => executeRequestLine(line, api));
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("oauth oracle requires a source module path");
	const oauthPage = (await import(pathToFileURL(`${modulePath}/auth/oauth/oauth-page.ts`).href)) as Record<
		string,
		unknown
	>;
	const pkce = (await import(pathToFileURL(`${modulePath}/auth/oauth/pkce.ts`).href)) as Record<string, unknown>;
	serveRequests({ ...oauthPage, ...pkce });
}

void main().catch((error: unknown) => {
	process.stderr.write(`oauth oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
