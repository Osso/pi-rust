import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { selectApprovedModule } from "./approved-delta.mjs";

const module = selectApprovedModule(process.argv[2], "packages/ai/src/image-models.ts");
const api = await import(pathToFileURL(module).href);
const provider = "openrouter";
const id = "google/gemini-2.5-flash-image";
function probeMutation(getter) {
	const original = structuredClone(api.getImageModel(provider, id));
	const model = getter === "single" ? api.getImageModel(provider, id) : api.getImageModels(provider).find((item) => item.id === id);
	if (!model) throw new Error("fixture image model missing");
	try {
		model.name = "caller-owned name";
		model.input.length = 0;
		model.output.length = 0;
		model.cost.input = 999;
		const single = api.getImageModel(provider, id);
		const listed = api.getImageModels(provider).find((item) => item.id === id);
		return { single: unchangedFields(single, original), list: unchangedFields(listed, original) };
	} finally {
		Object.assign(model, original);
	}
}
function unchangedFields(model, original) {
	return Object.fromEntries(["name", "input", "output", "cost"].map((key) => [key, JSON.stringify(model[key]) === JSON.stringify(original[key])]));
}
for await (const line of createInterface({ input: process.stdin })) {
	const { getter } = JSON.parse(line);
	const providers = api.getImageProviders();
	const models = api.getImageModels(provider);
	const originalProviders = [...providers];
	const count = models.length;
	providers.length = 0;
	models.length = 0;
	console.log(JSON.stringify({ mutation: probeMutation(getter),
		providersIndependent: JSON.stringify(api.getImageProviders()) === JSON.stringify(originalProviders),
		modelsIndependent: api.getImageModels(provider).length === count }));
}
