// Isolated synthetic credentials; the caller selects and verifies frozen or locked delta source.
import { serve } from "./config-auth-oracle-support.ts";
import { pathToFileURL } from "node:url";
import { readFileSync } from "node:fs";

async function main(): Promise<void> {
const sourceUrl = pathToFileURL(process.argv[2]);
const api = await import(sourceUrl.href);
const oauth = await import("@earendil-works/pi-ai/oauth");
serve(async (raw) => {
	const request = raw as { authPath: string; throws: boolean };
	let syncs = 0;
	const calls: string[] = [];
	oauth.registerOAuthProvider({
		id: "phase1-rotation", name: "Synthetic",
		login: async () => { throw new Error("unused"); },
		refreshToken: async (credential: {refresh: string}) => {
			if (credential.refresh !== "A") throw new Error("spent refresh token");
			calls.push("refresh A->B");
			return {refresh:"B",access:"new",expires:4_000_000_000_000,accountId:"synthetic"};
		},
		getApiKey: (credential: {access: string}) => credential.access,
	});
	const backend = {
		withLock: (fn: (current: string) => {result: unknown; next?: string}) => {
			calls.push(`sync ${++syncs}`);
			if (syncs > 1) throw new Error("rescue denied");
			return fn(readFileSync(request.authPath, "utf8")).result;
		},
		withLockAsync: async (fn: (current: string) => Promise<unknown>) => {
			calls.push("async callback");
			await fn(readFileSync(request.authPath, "utf8"));
			calls.push("after callback: write denied");
			throw new Error("locked write denied");
		},
	};
	const storage = api.AuthStorage.fromStorage(backend);
	let result: string | null = null;
	let error: string | null = null;
	let cause: string | null = null;
	try {
		result = await storage.getApiKey("phase1-rotation", {throwOnRefreshFailure:request.throws}) ?? null;
	} catch (caught) {
		error = (caught as Error).message;
		cause = ((caught as Error).cause as Error)?.message ?? null;
	} finally {
		oauth.unregisterOAuthProvider("phase1-rotation");
	}
	return {result,error,cause,calls,errors:storage.drainErrors().map((e: Error)=>e.message),memory:storage.getAll(),disk:JSON.parse(readFileSync(request.authPath,"utf8"))};
});
}
void main().catch(error => { process.stderr.write(`${String(error)}\n`); process.exitCode = 1; });
