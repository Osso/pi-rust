// Differential oracle for the bundled `codex-usage` extension.

import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type AnyRecord = Record<string, any>;

function tokenWithClaims(claims: unknown): string {
	return `header.${Buffer.from(JSON.stringify(claims)).toString("base64url")}.signature`;
}

function headersObject(headers: HeadersInit | undefined): Record<string, string> {
	const result: Record<string, string> = {};
	for (const [name, value] of new Headers(headers)) result[name.toLowerCase()] = value;
	return result;
}

function installFetch(responses: AnyRecord[], requests: AnyRecord[]): void {
	let index = 0;
	globalThis.fetch = (async (url: string, init?: RequestInit) => {
		requests.push({
			method: init?.method ?? "GET",
			url,
			headers: headersObject(init?.headers),
			body:
				typeof init?.body === "string"
					? init.body.replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g, "<uuid>")
					: null,
		});
		const response = responses[index++] ?? { status: 200, statusText: "", body: "{}" };
		return new Response(response.body, { status: response.status, statusText: response.statusText });
	}) as typeof fetch;
}

// Same credential the Rust harness installs: frozen AuthStorage, OAuth entry keyed by the active provider.
async function createAuthStorage(provider: string, token: string | null, input: AnyRecord): Promise<unknown> {
	const authStoragePath = resolve(dirname(process.argv[2] as string), "../../../src/core/auth-storage.ts");
	const { AuthStorage } = (await import(pathToFileURL(authStoragePath).href)) as AnyRecord;
	const data =
		token === null || input.runtimeOnly
			? {}
			: { [provider]: { type: "oauth", refresh: "refresh-token", access: token, expires: 9_999_999_999_999, email: input.storedEmail } };
	const storage = AuthStorage.inMemory(data);
	if (input.runtimeOnly && token !== null) storage.setRuntimeApiKey(provider, token);
	return storage;
}

void runOracle(async (api, request) => {
	const cases = (request.cases as AnyRecord[]) ?? [];
	const results: AnyRecord[] = [];
	for (const input of cases) {
		const output: AnyRecord = { name: input.name };
		if (input.kind === "map") {
			output.mapped = api.mapCodexUsagePayload(input.payload);
		} else if (input.kind === "format") {
			output.text = api.formatCodexUsage(input.usage);
		} else if (input.kind === "token") {
			const token = tokenWithClaims(input.claims);
			try {
				output.account = api.resolveCodexAccountInfo(token);
			} catch (error) {
				output.error = error instanceof Error ? error.message : String(error);
			}
		} else if (input.kind === "display_email") {
			const token = tokenWithClaims(input.claims);
			try {
				output.email = api.resolveCodexDisplayEmail(token, input.storedEmail ?? null);
			} catch (error) {
				output.error = error instanceof Error ? error.message : String(error);
			}
		} else if (input.kind === "completions") {
			let captured: AnyRecord | undefined;
			(api.default as (pi: AnyRecord) => void)({
				registerCommand(_name: string, value: AnyRecord) {
					captured = value;
				},
			});
			output.completions = captured?.getArgumentCompletions(input.prefix) ?? null;
		} else if (input.kind === "command") {
			output.error = null;
			const notifications: Array<[string, string | null]> = [];
			const messages: unknown[] = [];
			const requests: AnyRecord[] = [];
			installFetch(input.responses ?? [], requests);
			let command: AnyRecord | undefined;
			(api.default as (pi: AnyRecord) => void)({
				events: {
					emit(channel: string, request: AnyRecord) {
						const bridge = input.bridge;
						if (!bridge) return;
						if (channel === "claude-bridge:usage-all-request") {
							request.accounts = bridge.error !== undefined
								? Promise.reject(new Error(bridge.error))
								: Promise.resolve(bridge.accounts);
						} else if (channel === "claude-bridge:usage-request" && bridge.handled) {
							request.handled = (async () => {
								if (bridge.handledError !== undefined) throw new Error(bridge.handledError);
								request.ctx.ui.notify(`forwarded:${request.args}`, "info");
							})();
						}
					},
				},
				registerCommand(_name: string, value: AnyRecord) {
					command = value;
				},
				sendMessage(message: AnyRecord) {
					messages.push([message.customType, message.content, message.display]);
				},
			});
			const token = input.token === null ? null : tokenWithClaims(input.token);
			const ctx = {
				model: input.model ?? undefined,
				// The Rust harness session is idle between turns.
				isIdle: () => true,
				modelRegistry: {
					authStorage: await createAuthStorage(input.model?.provider ?? "openai-codex", token, input),
				},
				ui: {
					notify(message: string, type?: string) {
						notifications.push([message, type ?? null]);
					},
					confirm: async () => input.confirm ?? false,
				},
			};
			try {
				await command?.handler(input.args ?? "", ctx);
			} catch (error) {
				output.error = error instanceof Error ? error.message : String(error);
			}
			output.notifications = notifications;
			output.messages = messages;
			output.requests = requests;
		}
		results.push(output);
	}
	return { results };
}).catch((error: unknown) => {
	process.stderr.write(`codex-usage oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
