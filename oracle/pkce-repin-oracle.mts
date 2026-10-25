import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Request = { bytes: number[]; reject?: "random" | "digest"; raw?: boolean };
type PkceApi = { generatePKCE(): Promise<{ verifier: string; challenge: string }> };

const nativeCrypto = globalThis.crypto;
const api = (await import(pathToFileURL(process.argv[2]).href)) as PkceApi;
for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line) as Request;
    const calls: Array<{ operation: string; length?: number; algorithm?: string; bytes?: number[] }> = [];
    const rejection = request.raw ? "pkce-\ud800" : Object.assign(new TypeError("crypto-\udfff"), { code: "E_CRYPTO" });
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
                    calls.push({ operation: "digest", algorithm, bytes: Array.from(data) });
                    if (request.reject === "digest") throw rejection;
                    return nativeCrypto.subtle.digest(algorithm, data);
                },
            },
        },
    });
    try {
        console.log(JSON.stringify({ ok: true, value: await api.generatePKCE(), calls }));
    } catch (error) {
        const value = error instanceof Error
            ? { name: error.name, message: error.message, ...error }
            : error;
        console.log(JSON.stringify({ ok: false, value, calls }));
    }
}
