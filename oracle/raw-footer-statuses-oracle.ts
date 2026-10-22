import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import path from "node:path";

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const { FooterDataProvider } = await import(pathToFileURL(path.join(source, "core/footer-data-provider.ts")).href);
	const helpers = await import(pathToFileURL(path.join(source, "utils/footer-status.ts")).href);
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line);
			const raw = String.fromCharCode(...(request.units ?? []));
			let output: unknown;
			if (request.operation === "helpers") {
				const statuses = new Map([
					[`z${raw}`, `${raw}z\r\n\t  x`],
					[`a${raw}`, `${raw}\u0085`],
					["goal", ` \t${raw}\ufeff`],
				]);
				output = helpers.formatExtensionStatusTextLines(statuses);
			} else {
				const provider = new FooterDataProvider(request.directory);
				provider.setExtensionStatus(`z${raw}`, `first${raw}`);
				provider.setExtensionStatus(`a${raw}`, `second${raw}`);
				provider.setExtensionStatus(`z${raw}`, `updated${raw}`);
				const snapshots = [Array.from(provider.getExtensionStatuses())];
				provider.setExtensionStatus(`z${raw}`, undefined);
				snapshots.push(Array.from(provider.getExtensionStatuses()));
				provider.clearExtensionStatuses();
				snapshots.push(Array.from(provider.getExtensionStatuses()));
				provider.dispose();
				output = snapshots;
			}
			process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify(output) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
		}
	}
}
void main();
