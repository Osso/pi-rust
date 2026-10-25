import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	const { exportSessionToTerminal } = await import(pathToFileURL(modulePath).href);
	const { initTheme } = await import(pathToFileURL(join(dirname(modulePath), "../modes/interactive/theme/theme.ts")).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		try {
			initTheme(request.theme ?? "dark", false);
			const output = exportSessionToTerminal(request.path, request.columns);
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id: request.id, ok: false, error: { message: error instanceof Error ? error.message : String(error) } })}\n`);
		}
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
