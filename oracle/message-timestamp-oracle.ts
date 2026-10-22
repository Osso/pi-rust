import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const source = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		let timestamp = request.value;
		if (request.value === "nan") timestamp = Number.NaN;
		if (request.value === "invalid-date") {
			timestamp = source.createBranchSummaryMessage(
				"summary",
				"entry",
				"not a date",
			).timestamp;
		}
		if (request.value === "infinity") timestamp = Number.POSITIVE_INFINITY;
		if (request.value === "-infinity") timestamp = Number.NEGATIVE_INFINITY;
		if (request.value === "-0") timestamp = -0;
		const message = { ...request.message, timestamp };
		const wire = JSON.stringify(message);
		process.stdout.write(
			`${JSON.stringify({ wire, roundTrip: JSON.parse(wire), isNaN: Number.isNaN(timestamp) })}\n`,
		);
	}
}

void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
