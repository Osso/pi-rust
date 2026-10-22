import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		if (request.operation === "resolve") {
			const auth = api.envApiKeyAuth("Test key", ["KEY"]);
			const result = await auth.resolve({
				ctx: { env: async (name: string) => request.env[name] },
				credential: request.credential ?? undefined,
			});
			process.stdout.write(`${JSON.stringify(result ?? null)}\n`);
		} else {
			let loads = 0;
			const auth = api.lazyOAuth({
				name: "Failed loader",
				load: async () => {
					loads++;
					throw new Error("module unavailable");
				},
			});
			const errors: string[] = [];
			for (let i = 0; i < 2; i++) {
				try {
					await auth.toAuth({ type: "oauth", access: "token", refresh: "refresh", expires: 10 });
				} catch (error) {
					errors.push((error as Error).message);
				}
			}
			process.stdout.write(`${JSON.stringify({ loads, errors })}\n`);
		}
	}
}

void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
