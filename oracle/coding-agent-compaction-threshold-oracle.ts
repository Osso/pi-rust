import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const source = await import(pathToFileURL(process.argv[2]).href);
	const lines = createInterface({ input: process.stdin });
	for await (const line of lines) {
		const request = JSON.parse(line);
		try {
			const result = source.parseCompactionThresholdPercent(request.missing ? undefined : request.value);
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output: { result: result ?? null } })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output: { error: error instanceof Error ? error.message : String(error) } })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`${String(error)}\n`);
	process.exitCode = 1;
});
