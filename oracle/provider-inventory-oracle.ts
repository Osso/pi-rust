import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

// Separate factory-inventory proof; never used to export literal catalog data.
async function main(): Promise<void> {
	const source = process.argv[2];
	if (!source) throw new Error("provider inventory oracle requires providers/all.ts");
	const { builtinProviders } = await import(pathToFileURL(source).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line) as { id: number };
		const ids = builtinProviders().map((provider: { id: string }) => provider.id);
		process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output: ids })}\n`);
	}
}

void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
