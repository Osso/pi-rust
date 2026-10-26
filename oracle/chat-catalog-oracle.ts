import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { applyTerraDelta } from "./terra-delta.ts";

// Import only actual frozen literals: no factories, generators, or runtime barrel.
async function main(): Promise<void> {
	const source = process.argv[2];
	if (!source) throw new Error("chat catalog oracle requires models.generated.ts");
	const { MODELS } = await import(pathToFileURL(source).href);
	const barrel = readFileSync(source, "utf8");
	const imports = [...barrel.matchAll(/from "(\.\/providers\/[^\"]+\.models\.ts)"/g)].map((match) => match[1]);
	const files = [source, ...imports.map((file) => resolve(dirname(source), file))];
	const provenance = files.map((file) => ({
		path: `packages/ai/src/${file.slice(dirname(source).length + 1)}`,
		sha256: createHash("sha256").update(readFileSync(file)).digest("hex"),
	}));
	const models = { ...MODELS };
	await applyTerraDelta(models);
	const output = { models, provenance };
	if (process.argv[3] === "--export") {
		process.stdout.write(`${JSON.stringify(output)}\n`);
	} else {
		for await (const line of createInterface({ input: process.stdin })) {
			const request = JSON.parse(line) as { id: number };
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
