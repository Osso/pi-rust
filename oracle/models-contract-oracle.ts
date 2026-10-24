import { importSource, serve } from "./config-auth-oracle-support.ts";

type Model = { id: string };
type Provider = { getModels(): Model[] };
type Models = {
 setProvider(provider: Provider): void;
 getModels(provider: string): Model[];
 refresh(provider: string): Promise<void>;
};
type Source = {
 createModels(): Models;
 createProvider(input: Record<string, unknown>): Provider;
};

async function main(): Promise<void> {
 const { createModels, createProvider } = await importSource<Source>();
 serve(async (request) => {
  const models = createModels();
  const initial = request.initial;
  const refreshed = request.refreshed;
  const provider = createProvider({
   id: "dynamic", models: initial,
   auth: { apiKey: { name: "Ambient", resolve: async () => ({ auth: {} }) } },
   refreshModels: async () => refreshed,
   api: { stream() { throw new Error("must not stream"); }, streamSimple() { throw new Error("must not stream"); } },
  });
  models.setProvider(provider);
  const before = models.getModels("dynamic").map((model) => model.id);
  await models.refresh("dynamic");
  return { before, after: models.getModels("dynamic").map((model) => model.id) };
 });
}
void main();
