import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { ModelRegistry } = await import(pathToFileURL(`${root}/core/model-registry.ts`).href);
const { AuthStorage } = await import(pathToFileURL(`${root}/core/auth-storage.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		if (request.operation === "prototype") {
			const object: Record<string, unknown> = {};
			const properties = Object.getOwnPropertyNames(Object.prototype).map((name) => ({
				name, text: String(object[name]), truthy: Boolean(object[name]),
				numericNaN: Number.isNaN(Number(object[name])), type: typeof object[name],
			}));
			console.log(JSON.stringify({ ok: true, output: { node: process.version, v8: process.versions.v8, properties } }));
			continue;
		}
		const messages: string[] = [];
		const logins: unknown[] = [];
		const providers = request.providers ?? [];
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			runtimeHost: { session: { modelRegistry: {
				authStorage: {
					getOAuthProviders: () => providers,
					list: () => Object.keys(request.credentials ?? {}),
					get: (id: string) => request.credentials?.[id],
				},
				getAll: () => (request.modelProviders ?? []).map((provider: string) => ({ provider })),
				getProviderDisplayName: (id: string) => request.names[id],
			} } },
			showStatus: (message: string) => messages.push(message),
			showLoginDialog: async (id: string, name: string) => logins.push({ id, name }),
		});
		if (request.modelsPath) {
			const registry = ModelRegistry.create(AuthStorage.inMemory(request.credentials ?? {}), request.modelsPath);
			for (const provider of request.oauthProviders ?? []) {
				registry.registerProvider(provider.id, { oauth: {
					name: provider.name, login: async () => { throw new Error("unused login"); },
					refreshToken: async (credential: unknown) => credential,
					getApiKey: (credential: { access: string }) => credential.access,
				} });
			}
			receiver.runtimeHost.session.modelRegistry = registry;
		}
		let output: unknown;
		if (request.operation === "options") {
			const encode = (option: { id: string; name: string; authType: string }) => ({
				id: option.id, name: units(option.name), authType: option.authType,
			});
			output = {
				login: receiver.getLoginProviderOptions(request.authType).map(encode),
				logout: receiver.getLogoutProviderOptions().map(encode),
				...(request.modelsPath ? { error: receiver.runtimeHost.session.modelRegistry.getError() ?? null } : {}),
			};
		} else {
			await receiver.handleLoginCommand(String.fromCharCode(...request.provider));
			output = { messages: messages.map(units), logins };
		}
		if (request.modelsPath) {
			for (const provider of request.oauthProviders ?? []) receiver.runtimeHost.session.modelRegistry.unregisterProvider(provider.id);
		}
		console.log(JSON.stringify({ ok: true, output }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
