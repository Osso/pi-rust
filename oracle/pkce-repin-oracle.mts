import { createRequire, syncBuiltinESMExports } from "node:module";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Request = {
	bytes: number[];
	reject?: "random" | "digest" | "state";
	raw?: boolean;
	flow?: "anthropic" | "openai-codex";
};
type FlowAuth = {
	login(callbacks: {
		prompt(): Promise<string>;
		notify(): void;
	}): Promise<unknown>;
};
const nodeCrypto = createRequire(import.meta.url)("node:crypto") as {
	randomBytes(length: number): Buffer;
};
let fillState: (length: number) => Buffer = () => {
	throw new Error("state entropy fixture is not configured");
};
// The frozen Codex module caches its imported randomBytes callback; keep one
// stable callback and change only its per-request test state.
nodeCrypto.randomBytes = (length) => fillState(length);
syncBuiltinESMExports();

type PkceApi = {
	generatePKCE(): Promise<{ verifier: string; challenge: string }>;
};

const nativeCrypto = globalThis.crypto;
const api = (await import(pathToFileURL(process.argv[2]).href)) as PkceApi;
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as Request;
	const calls: Array<{
		operation: string;
		length?: number;
		algorithm?: string;
		bytes?: number[];
	}> = [];
	const effects: string[] = [];
	const rejection = request.raw
		? "pkce-\ud800"
		: Object.assign(new TypeError("crypto-\udfff"), { code: "E_CRYPTO" });
	fillState = (length) => {
		calls.push({ operation: "state", length });
		if (request.reject === "state") throw rejection;
		return Buffer.alloc(length);
	};
	globalThis.fetch = async () => {
		throw new Error("unexpected entropy fixture fetch");
	};
	Object.defineProperty(globalThis, "crypto", {
		configurable: true,
		value: {
			getRandomValues(bytes: Uint8Array) {
				calls.push({ operation: "random", length: bytes.length });
				if (request.reject === "random") throw rejection;
				bytes.set(request.bytes);
				return bytes;
			},
			subtle: {
				async digest(algorithm: string, data: Uint8Array) {
					calls.push({
						operation: "digest",
						algorithm,
						bytes: Array.from(data),
					});
					if (request.reject === "digest") throw rejection;
					return nativeCrypto.subtle.digest(algorithm, data);
				},
			},
		},
	});
	try {
		let value: unknown;
		if (request.flow) {
			const module = (await import(
				pathToFileURL(join(dirname(process.argv[2]), `${request.flow}.ts`)).href
			)) as { anthropicOAuth: FlowAuth; openaiCodexOAuth: FlowAuth };
			const auth =
				request.flow === "anthropic"
					? module.anthropicOAuth
					: module.openaiCodexOAuth;
			value = await auth.login({
				prompt: async () => {
					effects.push("prompt");
					return "browser";
				},
				notify: () => {
					effects.push("notify");
				},
			});
		} else {
			value = await api.generatePKCE();
		}
		console.log(JSON.stringify({ ok: true, value, calls, effects }));
	} catch (error) {
		const value =
			error instanceof Error
				? Object.assign({ name: error.name, message: error.message }, error)
				: error;
		console.log(JSON.stringify({ ok: false, value, calls, effects }));
	}
}
