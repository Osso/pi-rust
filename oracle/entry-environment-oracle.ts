import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const source = process.argv[2];
	if (!source) throw new Error("entry environment oracle requires a source module");
	// Invalid read-only export returns before filesystem/provider/runtime startup.
	process.argv = [process.execPath, source, "--export-terminal"];
	await import(pathToFileURL(source).href);
	process.exitCode = 0;
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line) as { id: number };
		const output = [process.env.PI_CODING_AGENT ?? null, process.env.AI_AGENT ?? null];
		process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output })}\n`);
	}
}

void main().catch((error: unknown) => {
	console.error("entry environment oracle failed:", error);
	process.exitCode = 1;
});
