import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { importSource, serve } from "./config-auth-oracle-support.ts";

type Source = {
	ModelsError: new (code: string, message: string) => Error;
	resolveProviderAuth(provider: unknown, model: unknown, store: unknown, context: unknown): Promise<unknown>;
};

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
	const { resolveProviderAuth, ModelsError } = await importSource<Source>();
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("auth resolve source required");
	const credentialStoreUrl = pathToFileURL(join(dirname(sourcePath), "credential-store.ts")).href;
	const { InMemoryCredentialStore } = await import(credentialStoreUrl);
	serve(async request => {
		if (request.operation === "imageMessage") return imageMessage(sourcePath, request, ModelsError);
		const store = new InMemoryCredentialStore();
		await store.modify("p1", async () => ({
			type: "oauth", refresh: "refresh", access: "old", expires: request.expires,
		}));
		const counts = { reads: 0, modifies: 0, refreshes: 0 };
		const observed = {
			read: async (id: string) => {
				counts.reads++;
				return store.read(id);
			},
			modify: async (id: string, modifier: unknown) => {
				counts.modifies++;
				return store.modify(id, modifier);
			},
			delete: (id: string) => store.delete(id),
		};
		const oauth = {
			name: "Fixture",
			login: async () => { throw new Error("must not login"); },
			refresh: async (credential: Record<string, unknown>) => {
				counts.refreshes++;
				return { ...credential, access: "new", expires: request.refreshedExpires };
			},
			toAuth: async (credential: Record<string, unknown>) => ({ apiKey: credential.access }),
		};
		const context = { env: async () => undefined, fileExists: async () => false };
		const result = await resolveProviderAuth({ id: "p1", auth: { oauth } }, request.model, observed, context);
		return { result, ...counts, stored: await store.read("p1") };
	});
}

void main();
