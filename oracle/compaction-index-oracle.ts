import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]!).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		let result: unknown;
		try {
			if (input.operation === "turn") {
				const value = api.findTurnStartIndex(
					input.entries,
					Number(input.entry),
					Number(input.start),
				);
				result = {
					ok: true,
					value,
					number: Object.is(value, -0) ? "-0" : String(value),
				};
			} else {
				const value = api.findCutPoint(
					input.entries,
					Number(input.start),
					Number(input.end),
					Number(input.keep),
				);
				result = {
					ok: true,
					value,
					firstNumber: Object.is(value.firstKeptEntryIndex, -0)
						? "-0"
						: String(value.firstKeptEntryIndex),
				};
			}
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = {
				ok: false,
				error: { name: error.name, message: error.message },
			};
		}
		console.log(JSON.stringify(result));
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
