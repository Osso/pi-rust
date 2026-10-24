import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { importSource, serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

type Source = {
	ModelsError: new (code: string, message: string, options?: { cause?: unknown }) => Error;
	resolveProviderAuth(provider: unknown, model: unknown, store: unknown, context: unknown): Promise<unknown>;
};
type Credential = Record<string, unknown>;
type Modifier = (current: Credential | undefined) => Promise<Credential | undefined>;
type Store = {
	read(id: string): Promise<Credential | undefined>;
	modify(id: string, modifier: Modifier): Promise<Credential | undefined>;
	delete(id: string): Promise<void>;
};
type Counts = { reads: number; modifies: number; refreshes: number };
type Failure = { kind: string; message: string; value?: unknown };

function createFixtureFailure(failure: Failure, ModelsError: Source["ModelsError"]): unknown {
	switch (failure.kind) {
		case "raw": return failure.value;
		case "native": return Object.assign(new TypeError(failure.message, { cause: false }), { code: "E_TEST" });
		case "models": return new ModelsError("stream", failure.message, { cause: false });
		case "spoof": return Object.assign(new Error(failure.message, { cause: false }), { name: "ModelsError", code: "stream" });
		case "nested": return new Error("outer", { cause: new ModelsError("stream", failure.message, { cause: false }) });
		case "defaultAbort": {
			const controller = new AbortController();
			controller.abort();
			return controller.signal.reason;
		}
		case "abortedFalse": return false;
		default: throw new Error(`unknown fixture failure ${failure.kind}`);
	}
}

function readBarrier(total: number): () => Promise<void> {
	let remaining = total;
	const waiters: Array<() => void> = [];
	return async () => {
		remaining--;
		if (remaining === 0) {
			for (const release of waiters) release();
			return;
		}
		await new Promise<void>(resolve => waiters.push(resolve));
	};
}

function observeStore(store: Store, counts: Counts, request: Record<string, unknown>, failure: unknown): Store {
	const waitForReads = readBarrier(2);
	return {
		read: async id => {
			counts.reads++;
			if (request.boundary === "read") throw failure;
			const current = await store.read(id);
			if (request.concurrent) await waitForReads();
			return current;
		},
		modify: async (id, modifier) => {
			counts.modifies++;
			if (request.boundary === "modify") throw failure;
			if (request.lockState === "loggedOut") await store.delete(id);
			if (request.lockState === "apiKey") await store.modify(id, async () => ({ type: "api_key", key: "changed" }));
			if (request.lockState === "refreshed") await store.modify(id, async () => ({
				type: "oauth", refresh: "refresh", access: "elsewhere", expires: request.refreshedExpires,
			}));
			return store.modify(id, modifier);
		},
		delete: id => store.delete(id),
	};
}

async function runAuthCase(api: Source, store: Store, request: Record<string, unknown>): Promise<unknown> {
	const counts = { reads: 0, modifies: 0, refreshes: 0 };
	const failure = request.failure ? createFixtureFailure(request.failure as Failure, api.ModelsError) : undefined;
	const initial = request.boundary === "api" ? { type: "api_key", key: "stored" } : {
		type: "oauth", refresh: "refresh", access: "old", expires: request.expires,
	};
	await store.modify("p1", async () => initial);
	const observed = observeStore(store, counts, request, failure);
	const oauth = {
		name: "Fixture",
		login: async () => { throw new Error("must not login"); },
		refresh: async (credential: Credential) => {
			counts.refreshes++;
			if (request.boundary === "refresh") throw failure;
			return { ...credential, access: "new", expires: request.refreshedExpires };
		},
		toAuth: async (credential: Credential) => {
			if (request.boundary === "derive") throw failure;
			return { apiKey: credential.access };
		},
	};
	const auth = request.boundary === "api" ? { apiKey: {
		name: "Fixture", resolve: async () => { throw failure; },
	} } : { oauth };
	const context = { env: async () => undefined, fileExists: async () => false };
	try {
		const resolve = () => api.resolveProviderAuth({ id: "p1", auth }, request.model, observed, context);
		const result = await (request.concurrent ? Promise.all([resolve(), resolve()]) : resolve());
		return { result, ...counts, stored: await store.read("p1") };
	} catch (error) {
		return { error: describeAbortReason(error), ...counts, stored: await store.read("p1") };
	}
}

async function imageMessage(sourcePath: string, request: Record<string, unknown>, ModelsError: Source["ModelsError"]): Promise<unknown> {
	const imagesUrl = pathToFileURL(join(dirname(sourcePath), "..", "images-models.ts")).href;
	const { createImagesModels, createImagesProvider } = await import(imagesUrl);
	const credentials = {
		read: async () => ({ type: "oauth", refresh: "refresh", access: "old", expires: 0 }),
		modify: async () => { throw new ModelsError("auth", request.message as string); },
	};
	const unused = async () => { throw new Error("must not run after modify failure"); };
	const oauth = { name: "Fixture", login: unused, refresh: unused, toAuth: unused };
	const models = createImagesModels({ credentials });
	models.setProvider(createImagesProvider({
		id: "p1", auth: { oauth }, models: [request.model], api: { generateImages: unused },
	}));
	const result = await models.generateImages(request.model, { input: [] });
	delete result.timestamp;
	return result;
}

async function main(): Promise<void> {
	const api = await importSource<Source>();
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("auth resolve source required");
	const credentialStoreUrl = pathToFileURL(join(dirname(sourcePath), "credential-store.ts")).href;
	const { InMemoryCredentialStore } = await import(credentialStoreUrl);
	serve(async request => {
		if (request.operation === "imageMessage") return imageMessage(sourcePath, request, api.ModelsError);
		return runAuthCase(api, new InMemoryCredentialStore(), request);
	});
}

void main();
