import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const module = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const provider = module[request.factory]();
		if (request.operation === "resolve") {
			const result = await provider.auth.apiKey.resolve({
				ctx: {
					env: async (name: string) => request.env[name],
					fileExists: async (path: string) =>
						(request.files ?? []).includes(path),
				},
				credential: request.credential ?? undefined,
			});
			process.stdout.write(`${JSON.stringify(result ?? null)}\n`);
		} else {
			process.stdout.write(
				`${JSON.stringify({
					id: provider.id,
					name: provider.name,
					baseUrl: provider.baseUrl ?? null,
					apiKeyName: provider.auth.apiKey?.name ?? null,
					canLogin: typeof provider.auth.apiKey?.login === "function",
					oauthName: provider.auth.oauth?.name ?? null,
					models: provider.getModels().map((model: { id: string }) => model.id),
				})}\n`,
			);
		}
	}
}

void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
