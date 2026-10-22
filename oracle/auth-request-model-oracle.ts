import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const result = await api.resolveProviderAuth({
			id: request.model.provider,
			auth: { apiKey: {
				name: "model-aware",
				resolve: async ({ model }: { model: { id: string; baseUrl: string } }) => ({
					auth: { apiKey: `${model.id}:${model.baseUrl}` },
				}),
			} },
		}, request.model, { read: async () => undefined }, {
			env: async () => undefined,
			fileExists: async () => false,
		});
		process.stdout.write(`${JSON.stringify(result)}\n`);
	}
}

void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
