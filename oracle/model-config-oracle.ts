// Oracle for `core/model-config.ts`: loads a models.json fixture with the real ModelConfig and
// returns the provider map plus the exact error text.
import { serve } from "./config-auth-oracle-support.ts";

type ModelConfigApi = {
	ModelConfig: {
		load(modelsJsonPath: string | undefined): Promise<{
			getProviderIds(): readonly string[];
			getProvider(id: string): unknown;
			getError(): string | undefined;
		}>;
	};
};

async function main(): Promise<void> {
	const { pathToFileURL } = await import("node:url");
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("model config oracle requires a source module path");
	const api = (await import(pathToFileURL(sourcePath).href)) as ModelConfigApi;

	serve(async (request) => {
		const config = await api.ModelConfig.load(request.path as string | undefined);
		const ids = [...config.getProviderIds()];
		return {
			providerIds: ids,
			providers: Object.fromEntries(ids.map((id) => [id, config.getProvider(id)])),
			error: config.getError() ?? null,
		};
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`model config oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
