import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { ModelRegistry } = await import(pathToFileURL(`${root}/core/model-registry.ts`).href);
const { AuthStorage } = await import(pathToFileURL(`${root}/core/auth-storage.ts`).href);
const { Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		themes.initTheme("dark", false);
		const chatContainer = new Container();
		let count: number | undefined;
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
			chatContainer, ui: { requestRender: () => {} },
			footerDataProvider: { setAvailableProviderCount: (value: number) => { count = value; } },
			footer: { invalidate: () => {} }, updateEditorBorderColor: () => {},
			showLoginDialog: async (id: string, name: string) => logins.push({ id, name }),
		});
		if (request.modelsPath) {
			const storage = request.authPath ? AuthStorage.create(request.authPath) : AuthStorage.inMemory(request.credentials ?? {});
			const registry = ModelRegistry.create(storage, request.modelsPath);
			for (const provider of request.oauthProviders ?? []) {
				registry.registerProvider(provider.id, { oauth: {
					name: provider.name, login: async () => { throw new Error("unused login"); },
					refreshToken: async (credential: unknown) => credential,
					getApiKey: (credential: { access: string }) => credential.access,
				} });
			}
			for (const provider of request.providerOverrides ?? []) registry.registerProvider(provider.id, provider.config);
			receiver.runtimeHost.session.modelRegistry = registry;
		}
		let output: unknown;
		if (request.operation === "complete") {
			const session = receiver.runtimeHost.session;
			let setterError: unknown;
			if (request.failedMetadataPath) {
				const { Agent } = await import(pathToFileURL(`${root}/../../agent-core/src/index.ts`).href);
				const { AgentSession } = await import(pathToFileURL(`${root}/core/agent-session.ts`).href);
				const { SessionManager } = await import(pathToFileURL(`${root}/core/session-manager.ts`).href);
				const { SettingsManager } = await import(pathToFileURL(`${root}/core/settings-manager.ts`).href);
				const { createTestResourceLoader } = await import(pathToFileURL(`${root}/../test/utilities.ts`).href);
				const manager = SessionManager.create(request.selectionRoot, `${request.selectionRoot}/source-sessions`);
				const real = new AgentSession({
					agent: new Agent({ initialState: { model: request.previous, tools: [] } }),
					sessionManager: manager, modelRegistry: session.modelRegistry,
					settingsManager: SettingsManager.inMemory({ warnings: { anthropicExtraUsage: false } }),
					cwd: request.selectionRoot, agentDir: request.selectionRoot, resourceLoader: createTestResourceLoader(),
				});
				try { manager.setMetadataControlDbPath(request.failedMetadataPath); } catch { /* Retain the failed configured path. */ }
				receiver.runtimeHost.session = real;
				try { await real.setModel(request.selectionModel); } catch (error) {
					if (!(error instanceof Error)) throw error;
					setterError = { name: error.name, message: error.message, ...Object.fromEntries(Object.entries(error)) };
				}
				real.agent.state.model = request.previous;
			} else {
				Object.assign(session, {
					model: request.previous, scopedModels: request.scopedModels ?? [],
					settingsManager: { getWarnings: () => ({ anthropicExtraUsage: false }) },
					setModel: async (model: unknown) => { session.model = model; },
				});
			}
			await receiver.completeProviderAuthentication(request.provider, String.fromCharCode(...request.name), request.authType, request.previous);
			output = { model: receiver.runtimeHost.session.model, count, chat: chatContainer.render(200).map(units), ...(request.failedMetadataPath ? { setterError } : {}) };
			if (request.failedMetadataPath) {
				receiver.runtimeHost.session.sessionManager.setMetadataControlDbPath(undefined);
				receiver.runtimeHost.session.dispose();
			}
		} else if (request.operation === "warning" || request.operation === "warningCompletion") {
			const session = receiver.runtimeHost.session;
			session.model = request.model;
			session.settingsManager = { getWarnings: () => request.warningSettings ?? {} };
			receiver.anthropicSubscriptionWarningShown = request.alreadyShown ?? false;
			if (request.runtimeKey !== undefined && request.runtimeKey !== null) session.modelRegistry.authStorage.setRuntimeApiKey("anthropic", request.runtimeKey);
			let lookupFailed = false;
			if (request.inspectLookup) {
				try { await session.modelRegistry.getApiKeyForProvider("anthropic"); } catch { lookupFailed = true; }
			}
			if (request.operation === "warningCompletion") {
				session.scopedModels = [];
				await receiver.completeProviderAuthentication("anthropic", "Anthropic", "api_key", request.model);
				const snapshot = () => ({ shown: receiver.anthropicSubscriptionWarningShown, chat: chatContainer.render(200).map(units) });
				const before = snapshot();
				await new Promise<void>((resolve) => setImmediate(resolve));
				output = { before, after: snapshot() };
			} else {
				for (let index = 0; index < 2; index++) await receiver.maybeWarnAboutAnthropicSubscriptionAuth(request.useDefault ? undefined : request.model);
				output = { shown: receiver.anthropicSubscriptionWarningShown, lookupFailed, chat: chatContainer.render(200).map(units) };
			}
		} else if (request.operation === "options") {
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
			output = { chat: chatContainer.render(200).map(units), logins };
		}
		if (request.modelsPath) {
			for (const provider of [...(request.oauthProviders ?? []), ...(request.providerOverrides ?? [])]) receiver.runtimeHost.session.modelRegistry.unregisterProvider(provider.id);
		}
		console.log(JSON.stringify({ ok: true, output }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
