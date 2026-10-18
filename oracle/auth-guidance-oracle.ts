// Oracle for `core/auth-guidance.ts`. `PI_PACKAGE_DIR` is set on the process so the packaged docs
// path matches the Rust runtime's package directory.
import { importSource, serve } from "./config-auth-oracle-support.ts";

type AuthGuidanceApi = {
	getProviderLoginHelp(): string;
	formatNoModelsAvailableMessage(): string;
	formatNoModelSelectedMessage(): string;
	formatNoApiKeyFoundMessage(provider: string): string;
};

async function main(): Promise<void> {
	const api = await importSource<AuthGuidanceApi>();
	serve((request) => {
		const providers = (request.providers as string[]) ?? [];
		return {
			loginHelp: api.getProviderLoginHelp(),
			noModels: api.formatNoModelsAvailableMessage(),
			noModelSelected: api.formatNoModelSelectedMessage(),
			noApiKey: providers.map((provider) => api.formatNoApiKeyFoundMessage(provider)),
		};
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`auth guidance oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
