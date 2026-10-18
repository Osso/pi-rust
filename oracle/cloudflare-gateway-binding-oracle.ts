import { pathToFileURL } from "node:url";

type Call = {
	url: string;
	method?: string;
	headers?: Record<string, string>;
	body?: string;
	signal?: boolean;
};

type Scenario = {
	id: number;
	baseUrl: string;
	gateway: string;
	calls: Call[];
	response?: {
		status?: number;
		statusText?: string;
		headers?: Record<string, string>;
		body?: string;
	};
};

type UniversalRequest = {
	provider: string;
	endpoint: string;
	headers: Record<string, string>;
	query: unknown;
};

type BindingApi = {
	CLOUDFLARE_GATEWAY_BINDING_AUTH_SENTINEL: string;
	createGatewayBindingFetch(options: {
		binding: unknown;
		baseUrl: string;
		gateway: string;
	}): (input: string, init?: RequestInit) => Promise<Response>;
};

function fakeBinding(makeResponse: () => Response, runs: unknown[]) {
	return {
		gateway(gatewayId: string) {
			return {
				run(data: UniversalRequest, options?: { signal?: AbortSignal }) {
					runs.push({
						gateway: gatewayId,
						provider: data.provider,
						endpoint: data.endpoint,
						headers: data.headers,
						query: data.query,
						hasSignal: options?.signal !== undefined,
					});
					return Promise.resolve(makeResponse());
				},
			};
		},
	};
}

async function execute(scenario: Scenario, api: BindingApi): Promise<unknown> {
	const responseInit = scenario.response ?? {};
	const makeResponse = () =>
		new Response(responseInit.body ?? "{}", {
			status: responseInit.status ?? 200,
			statusText: responseInit.statusText ?? "OK",
			headers: { "content-type": "text/plain", ...(responseInit.headers ?? {}) },
		});
	const runs: unknown[] = [];
	const fetch = api.createGatewayBindingFetch({
		binding: fakeBinding(makeResponse, runs),
		baseUrl: scenario.baseUrl,
		gateway: scenario.gateway,
	});

	const calls: unknown[] = [];
	for (const call of scenario.calls) {
		const controller = new AbortController();
		const init: RequestInit = { method: call.method ?? "GET" };
		if (call.headers !== undefined) init.headers = call.headers;
		if (call.body !== undefined) init.body = call.body;
		if (call.signal === true) init.signal = controller.signal;
		const runCount = runs.length;
		try {
			const response = await fetch(call.url, init);
			calls.push({
				ok: true,
				run: runs.length > runCount ? runs[runs.length - 1] : null,
				response: {
					status: response.status,
					statusText: response.statusText,
					headers: Object.fromEntries(response.headers),
					body: await response.text(),
				},
			});
		} catch (error) {
			calls.push({
				ok: false,
				error: error instanceof Error ? error.message : String(error),
			});
		}
	}

	return { id: scenario.id, calls };
}

function serve(api: BindingApi): void {
	let buffer = "";
	process.stdin.setEncoding("utf8");
	process.stdin.on("data", (chunk: string) => {
		buffer += chunk;
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			void execute(JSON.parse(line) as Scenario, api).then((response) => {
				process.stdout.write(`${JSON.stringify(response)}\n`);
			});
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("gateway binding oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as BindingApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`gateway binding oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
