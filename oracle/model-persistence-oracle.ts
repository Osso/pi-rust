// Same-file models.json interchange. The frozen registry has no configuration writer;
// writeFileSync below is fixture authorship, not a simulated registry persistence API.
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

type Registry = {
	getAll(): Array<{ provider: string }>;
	getError(): string | undefined;
	refresh(): void;
};
type RegistryModule = {
	ModelRegistry: { create(auth: unknown, path: string): Registry };
};
type AuthModule = { AuthStorage: { inMemory(data: object): unknown } };

async function main(): Promise<void> {
	const source = process.argv[2];
	if (!source) throw new Error("model persistence oracle requires frozen core directory");
	const { ModelRegistry } = await import(pathToFileURL(`${source}/model-registry.ts`).href) as RegistryModule;
	const { AuthStorage } = await import(pathToFileURL(`${source}/auth-storage.ts`).href) as AuthModule;
	globalThis.fetch = async () => { throw new Error("models.json must not fetch"); };
	serve((request) => {
		if (typeof request.path !== "string") throw new Error("models.json path required");
		if (request.document !== undefined) writeFileSync(request.path, JSON.stringify(request.document));
		const registry = ModelRegistry.create(AuthStorage.inMemory({}), request.path);
		const snapshot = () => {
			const models = registry.getAll().filter((model) => model.provider === "reciprocal-fixture");
			return { error: registry.getError() ?? null, models, wire: JSON.stringify(models) };
		};
		const before = snapshot();
		registry.refresh();
		return { before, after: snapshot(), file: readFileSync(request.path, "utf8") };
	});
}
void main().catch((error: unknown) => {
	process.stderr.write(`model persistence oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
