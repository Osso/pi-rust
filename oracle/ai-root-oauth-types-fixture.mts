import type {
	OAuthCredentials,
	OAuthProvider,
	OAuthProviderId,
	OAuthProviderInfo,
	OAuthProviderInterface,
} from "@pi-oracle-source/ai/src/index.ts";

declare const credentials: OAuthCredentials;
declare const provider: OAuthProviderInterface;
const apiKey: string = provider.getApiKey(credentials);
const id: OAuthProviderId = "github-copilot";
const alias: OAuthProvider = id;
const info: OAuthProviderInfo = {
	id: alias,
	name: "GitHub Copilot",
	available: true,
};
export { apiKey, info };
