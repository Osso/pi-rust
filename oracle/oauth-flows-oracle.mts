import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ScriptedResponse = { status: number; body: string };
type PromptAnswer =
	| { text: string }
	| { error: string }
	| { code: string; fromAuthUrl: true }
	| { pending: true };
type Scenario = {
	responses: ScriptedResponse[];
	prompts: PromptAnswer[];
	callback?: { code: string };
};

type Request =
	| { id: RequestId; operation: "flow"; flow: string; scenario: Scenario }
	| {
			id: RequestId;
			operation: "refresh";
			flow: string;
			credential: Record<string, unknown>;
			responses: ScriptedResponse[];
	  };

type OracleResponse =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

const NOW = 1_750_000_000_000;
const FIXED_UUID = "00000000-0000-4000-8000-000000000000";

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function deterministicBytes(length: number): Uint8Array {
	const bytes = new Uint8Array(length);
	for (let index = 0; index < length; index++) bytes[index] = (index % 255) + 1;
	return bytes;
}

function installDeterminism(): void {
	Date.now = () => NOW;
	const realCrypto = globalThis.crypto;
	Object.defineProperty(globalThis, "crypto", {
		configurable: true,
		value: {
			subtle: realCrypto.subtle,
			getRandomValues<T extends ArrayBufferView>(array: T): T {
				const bytes = deterministicBytes(array.byteLength);
				new Uint8Array(array.buffer, array.byteOffset, array.byteLength).set(
					bytes,
				);
				return array;
			},
			randomUUID: () => FIXED_UUID,
		},
	});
	const require = createRequire(import.meta.url);
	const nodeCrypto = require("node:crypto");
	nodeCrypto.randomBytes = (length: number) =>
		Buffer.from(deterministicBytes(length));
}

function requestUrl(input: unknown): string {
	if (typeof input === "string") return input;
	if (input instanceof URL) return input.toString();
	if (input instanceof Request) return input.url;
	throw new Error(`unsupported fetch input: ${String(input)}`);
}

function normalizeHeaders(headers: unknown): Record<string, string> {
	if (headers === undefined) return {};
	if (headers instanceof Headers) {
		return Object.fromEntries(
			Array.from(headers.entries(), ([name, value]) => [
				name.toLowerCase(),
				value,
			]),
		);
	}
	return Object.fromEntries(
		Object.entries(headers as Record<string, string>).map(([name, value]) => [
			name.toLowerCase(),
			String(value),
		]),
	);
}

const nativeFetch = globalThis.fetch;

function installFetch(
	scenario: Scenario,
	recorder: Record<string, unknown>,
): void {
	const responses = [...scenario.responses];
	const requests = recorder.requests as Array<Record<string, unknown>>;
	globalThis.fetch = (async (
		input: unknown,
		init?: RequestInit,
	): Promise<Response> => {
		const url = requestUrl(input);
		if (
			url.startsWith("http://127.0.0.1") ||
			url.startsWith("http://localhost")
		) {
			return nativeFetch(input as string, init);
		}
		requests.push({
			method: init?.method ?? "GET",
			url,
			headers: normalizeHeaders(init?.headers),
			body: init?.body === undefined ? "" : String(init.body),
		});
		const response = responses.shift();
		if (!response) throw new Error(`unexpected fetch: ${url}`);
		return new Response(response.body, {
			status: response.status,
			headers: { "content-type": "application/json" },
		});
	}) as typeof fetch;
}

async function hitCallback(
	authUrl: string,
	code: string,
): Promise<{ status: number; body: string }> {
	const url = new URL(authUrl);
	const callbackUrl = url.searchParams.get("callback_url");
	let target: URL;
	if (callbackUrl) {
		target = new URL(callbackUrl);
		target.searchParams.set("code", code);
	} else {
		const redirectUri = url.searchParams.get("redirect_uri");
		const state = url.searchParams.get("state");
		if (!redirectUri || !state)
			throw new Error("auth URL is missing redirect_uri/state");
		target = new URL(redirectUri);
		target.searchParams.set("code", code);
		target.searchParams.set("state", state);
	}
	const response = await nativeFetch(target);
	return { status: response.status, body: await response.text() };
}

function createInteraction(
	scenario: Scenario,
	recorder: Record<string, unknown>,
): Record<string, unknown> {
	const answers = [...scenario.prompts];
	let authUrl: string | undefined;
	return {
		signal: new AbortController().signal,
		notify(event: Record<string, unknown>) {
			(recorder.events as Array<Record<string, unknown>>).push(event);
			if (event.type !== "auth_url") return;
			authUrl = event.url as string;
			if (scenario.callback) {
				recorder.callback = hitCallback(authUrl, scenario.callback.code).catch(
					(error: unknown) => ({
						status: 0,
						body: normalizeError(error).message,
					}),
				);
			}
		},
		async prompt(prompt: Record<string, unknown>): Promise<string> {
			(recorder.prompts as Array<Record<string, unknown>>).push({
				type: prompt.type,
				message: prompt.message,
				...(prompt.placeholder === undefined
					? {}
					: { placeholder: prompt.placeholder }),
				...(prompt.options === undefined
					? {}
					: {
							options: (prompt.options as Array<Record<string, unknown>>).map(
								(option) => ({ id: option.id, label: option.label }),
							),
						}),
			});
			const answer = answers.shift();
			if (!answer) return new Promise<string>(() => {});
			if ("pending" in answer) return new Promise<string>(() => {});
			if ("error" in answer) throw new Error(answer.error);
			if ("fromAuthUrl" in answer) {
				if (!authUrl)
					throw new Error("manual answer requested before auth_url");
				const url = new URL(authUrl);
				const redirectUri = url.searchParams.get("redirect_uri");
				const state = url.searchParams.get("state");
				if (!redirectUri || !state)
					throw new Error("auth URL is missing redirect_uri/state");
				return `${redirectUri}?code=${answer.code}&state=${state}`;
			}
			return answer.text;
		},
	};
}

async function loadFlow(
	modulePath: string,
	flow: string,
): Promise<Record<string, unknown>> {
	const module = (await import(
		pathToFileURL(`${modulePath}/utils/oauth/${flow}.ts`).href
	)) as Record<string, unknown>;
	return module;
}

function flowInstance(
	module: Record<string, unknown>,
	flow: string,
): Record<string, unknown> {
	const names: Record<string, string> = {
		anthropic: "anthropicOAuth",
		"openai-codex": "openaiCodexOAuth",
		openrouter: "openRouterOAuth",
	};
	return module[names[flow]] as Record<string, unknown>;
}

async function runFlow(
	modulePath: string,
	flow: string,
	scenario: Scenario,
): Promise<unknown> {
	const recorder: Record<string, unknown> = {
		events: [],
		prompts: [],
		requests: [],
	};
	installFetch(scenario, recorder);
	const module = await loadFlow(modulePath, flow);
	const oauth = flowInstance(module, flow);
	const interaction = createInteraction(scenario, recorder);
	let outcome: Record<string, unknown>;
	try {
		const credential = await (
			oauth.login as (interaction: unknown) => Promise<unknown>
		)(interaction);
		outcome = { ok: true, credential };
	} catch (error) {
		outcome = { ok: false, error: normalizeError(error) };
	}
	if (recorder.callback) recorder.callback = await recorder.callback;
	return {
		...outcome,
		events: recorder.events,
		prompts: recorder.prompts,
		requests: recorder.requests,
		callback: recorder.callback ?? null,
	};
}

async function runRefresh(
	modulePath: string,
	flow: string,
	credential: Record<string, unknown>,
	responses: ScriptedResponse[],
): Promise<unknown> {
	const recorder: Record<string, unknown> = {
		events: [],
		prompts: [],
		requests: [],
	};
	installFetch({ responses, prompts: [] }, recorder);
	const module = await loadFlow(modulePath, flow);
	const oauth = flowInstance(module, flow);
	try {
		const refreshed = await (
			oauth.refresh as (
				credential: unknown,
				signal: AbortSignal,
			) => Promise<unknown>
		)(credential, new AbortController().signal);
		return { ok: true, credential: refreshed, requests: recorder.requests };
	} catch (error) {
		return {
			ok: false,
			error: normalizeError(error),
			requests: recorder.requests,
		};
	}
}

async function execute(request: Request, modulePath: string): Promise<unknown> {
	if (request.operation === "flow")
		return runFlow(modulePath, request.flow, request.scenario);
	return runRefresh(
		modulePath,
		request.flow,
		request.credential,
		request.responses,
	);
}

async function executeLine(line: string, modulePath: string): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(
			`${JSON.stringify({ id, ok: true, output: await execute(request, modulePath) })}\n`,
		);
	} catch (error) {
		const response: OracleResponse = {
			id,
			ok: false,
			error: normalizeError(error),
		};
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serve(modulePath: string): void {
	let buffer = "";
	let queue = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += chunk.toString();
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(() => executeLine(line, modulePath));
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined)
		throw new Error("oauth flows oracle requires a source module path");
	installDeterminism();
	serve(modulePath);
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`oauth flows oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
