// Compile the real frozen public interfaces, not a local shape substitute.
import type {
	ApiKeyAuth,
	AuthContext,
	AuthEvent,
	AuthLoginCallbacks,
	CredentialStore,
	OAuthAuth,
} from "@pi-oracle-source/ai/src/auth/types.ts" with { "resolution-mode": "import" };

export const storeKeys: Record<keyof CredentialStore, true> = {
	read: true,
	modify: true,
	delete: true,
};
export const apiKeyKeys: Record<keyof ApiKeyAuth, true> = {
	name: true,
	login: true,
	resolve: true,
};
export const oauthKeys: Record<keyof OAuthAuth, true> = {
	name: true,
	login: true,
	refresh: true,
	toAuth: true,
};
export const eventTags: Record<AuthEvent["type"], true> = {
	auth_url: true,
	device_code: true,
	progress: true,
};
export const context: AuthContext = {
	env: async () => undefined,
	fileExists: async () => false,
};
export const store: CredentialStore = {
	read: async () => undefined,
	modify: async (_provider, modify) => modify(undefined),
	delete: async () => {},
};
export const keyAuth: ApiKeyAuth = {
	name: "name\ud800",
	resolve: async ({ model }) => ({ auth: { apiKey: model.id } }),
	login: async (callbacks: AuthLoginCallbacks) => ({
		type: "api_key",
		key: await callbacks.prompt({ type: "secret", message: "Enter name\ud800" }),
	}),
};
export const oauth: OAuthAuth = {
	name: "name\ud800",
	login: async () => ({ type: "oauth", access: "a", refresh: "r", expires: 10 }),
	refresh: async (credential) => credential,
	toAuth: async (credential) => ({ apiKey: credential.access }),
};
