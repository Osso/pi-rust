// Frozen public AuthStorage API. Synthetic, isolated file paths supplied by the sandboxed test.
import { serve } from "./config-auth-oracle-support.ts";

type Storage = {
	get(provider: string): unknown;
	set(provider: string, credential: unknown): void;
	remove(provider: string): void;
	list(): string[];
	getAll(): unknown;
	drainErrors(): Error[];
};

async function main(): Promise<void> {
	const { readFileSync, existsSync } = await import("node:fs");
	const { pathToFileURL } = await import("node:url");
	const sourceUrl = pathToFileURL(process.argv[2]);
	const api = await import(sourceUrl.href);
	const oauth = await import(
		new URL("../../../ai/src/utils/oauth/index.ts", sourceUrl).href
	);
	serve(async (raw) => {
		const request = raw as {
			id: unknown;
			op: string;
			authPath: string;
			provider: string;
			credential: unknown;
			credentialJson?: string;
			providerJson?: string;
		};
		if (request.op === "rotate-in-place") {
			oauth.registerOAuthProvider({
				id: "fake", name: "Synthetic in-place rotation",
				login: async () => { throw new Error("unused login"); },
				refreshToken: async (credential: Record<string, unknown>) => {
					credential.refresh = "B\udfff";
					credential.access = "new";
					credential.expires = 4_000_000_000_000;
					return credential;
				},
				getApiKey: (credential: { access: string }) => credential.access,
			});
			try {
				const storage = api.AuthStorage.create(request.authPath);
				await storage.getApiKey("fake");
				return { credential: JSON.stringify(storage.get("fake")), file: readFileSync(request.authPath, "utf8") };
			} finally { oauth.unregisterOAuthProvider("fake"); }
		}
		if (request.op === "backend-rejection") {
			const backend = new api.InMemoryAuthStorageBackend();
			backend.withLock(() => ({ result: undefined, next: "{}" }));
			let thrown: unknown;
			try {
				await backend.withLockAsync(async () => { throw raw.reason; });
			} catch (error) { thrown = error; }
			const after = backend.withLock((current: string) => ({ result: JSON.parse(current) }));
			return { thrown, after };
		}
		if (request.op === "rotation-all-sync-fail") {
			const calls: string[] = [];
			let syncs = 0;
			oauth.registerOAuthProvider({
				id: "phase1-rotation",
				name: "Synthetic",
				login: async () => {
					throw new Error("unused");
				},
				refreshToken: async () => {
					calls.push("refresh A->B");
					return { refresh: "B", access: "new", expires: 4_000_000_000_000 };
				},
				getApiKey: (credential: { access: string }) => credential.access,
			});
			const backend = {
				withLock: (
					fn: (current: string) => { result: unknown; next?: string },
				) => {
					calls.push(`sync ${++syncs}`);
					if (syncs > 1) throw new Error("sync denied");
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
			const result = await storage.getApiKey("phase1-rotation", {
				throwOnRefreshFailure: true,
			});
			oauth.unregisterOAuthProvider("phase1-rotation");
			return {
				result,
				calls,
				errors: storage.drainErrors().map((error: Error) => error.message),
				memory: storage.getAll(),
				file: readFileSync(request.authPath, "utf8"),
			};
		}
		if (request.op === "snapshot-reloads") {
			let content = JSON.stringify(raw.before);
			let reads = 0;
			const backend = {
				withLock: (
					fn: (current: string) => { result: unknown; next?: string },
				) => {
					reads++;
					const result = fn(content);
					if (result.next !== undefined) content = result.next;
					return result.result;
				},
				withLockAsync: async () => {
					throw new Error(
						"Snapshot reload must use the synchronous backend boundary",
					);
				},
			};
			const first = api.AuthStorage.fromStorage(backend);
			const second = api.AuthStorage.fromStorage(backend);
			const sample = () => ({
				first: first.getAll(),
				second: second.getAll(),
				reads,
			});
			const initial = sample();
			content = JSON.stringify(raw.after);
			const externalEdit = sample();
			first.reload();
			const firstReload = sample();
			second.reload();
			return { initial, externalEdit, firstReload, secondReload: sample() };
		}
		const storage: Storage = api.AuthStorage.create(request.authPath);
		if (request.op === "async-lock-read") {
			const backend = new api.FileAuthStorageBackend(request.authPath);
			if (raw.readyPath) {
				const { writeFileSync } = await import("node:fs");
				writeFileSync(String(raw.readyPath), "");
			}
			return backend.withLockAsync(async (current: string | undefined) => ({
				result: {
					credential: JSON.parse(current ?? "{}")[request.provider] ?? null,
				},
			}));
		}
		if (request.op === "raw-get")
			return {
				credential: storage.get(request.provider) ?? null,
				all: storage.getAll(),
				providers: storage.list(),
				errors: storage.drainErrors().map((error) => error.message),
			};
		if (request.op === "lossless") {
			const provider = request.providerJson
				? JSON.parse(request.providerJson)
				: request.provider;
			if (request.credentialJson)
				storage.set(provider, JSON.parse(request.credentialJson));
			return {
				credential: JSON.stringify(storage.get(provider)),
				all: JSON.stringify(storage.getAll()),
				entries: JSON.stringify(storage.list()),
				file: readFileSync(request.authPath, "utf8"),
			};
		}
		if (request.op === "error-identities") {
			return storage.drainErrors().map((error) => ({ name: error.name, message: error.message, constructor: error.constructor.name }));
		}
		if (request.op === "errors")
			return { errors: storage.drainErrors().map((error) => error.message) };
		if (request.op === "read")
			return { credential: storage.get(request.provider) ?? null };
		if (request.op === "modify" || request.op === "modify-file")
			storage.set(request.provider, request.credential);
		if (request.op === "modify-file")
			return { file: readFileSync(request.authPath, "utf8") };
		if (request.op === "delete") storage.remove(request.provider);
		return {
			credential: storage.get(request.provider) ?? null,
			entries: storage.list(),
			all: storage.getAll(),
			errors: storage.drainErrors().map((error) => error.message),
			file: existsSync(request.authPath)
				? readFileSync(request.authPath, "utf-8")
				: null,
		};
	});
}
void main().catch((error) => {
	process.stderr.write(`${String(error)}\n`);
	process.exitCode = 1;
});
