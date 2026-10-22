import { createInterface } from "node:readline";
import path from "node:path";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const themeModule = await import(pathToFileURL(path.join(source, "modes/interactive/theme/theme.ts")).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const themePath = String.fromCharCode(...request.pathUnits);
		const raw = String.fromCharCode(...request.units);
		let result: unknown;
		try {
			const theme = themeModule.loadThemeFromPath(themePath, "truecolor");
			result = [theme.name, theme.sourcePath, theme.fg("accent", raw)];
		} catch (error) {
			result = error instanceof Error ? error.message : String(error);
		}
		process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify(result) })}\n`);
	}
}
void main();
