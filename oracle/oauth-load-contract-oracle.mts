import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

const modulePath = process.argv[2];
const source = await import(pathToFileURL(modulePath).href);
const imageModule = await import(
	pathToFileURL(
		join(dirname(modulePath), "../../providers/openrouter-images.ts"),
	).href
);
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	if (request.operation === "images") {
		const provider = imageModule.openrouterImagesProvider();
		process.stdout.write(
			`${JSON.stringify({ id: provider.id, name: provider.name, authKeys: Object.keys(provider.auth), apiKeyName: provider.auth.apiKey.name, oauth: provider.auth.oauth ?? null })}\n`,
		);
		continue;
	}
	const records = [];
	for (const name of Object.keys(source).sort()) {
		const auth = await source[name]();
		records.push({
			name,
			authName: auth.name,
			authKeys: Object.keys(auth).sort(),
			result: await auth.toAuth({
				type: "oauth",
				access: "plain-token",
				refresh: "r",
				expires: 10,
			}),
		});
	}
	process.stdout.write(`${JSON.stringify(records)}\n`);
}
