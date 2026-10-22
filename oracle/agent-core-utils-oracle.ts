import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
const moduleUrl = pathToFileURL(process.argv[2]!);
const truncate = await import(moduleUrl.href);
const shell = await import(new URL("./shell-output.ts", moduleUrl).href);
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { units: number[]; maxLines: number | string; maxBytes: number | string; maxChars: number | string; formatBytes?: string };
	const content = String.fromCharCode(...request.units);
	const options = { maxLines: Number(request.maxLines), maxBytes: Number(request.maxBytes) };
	const output = {
		head: truncate.truncateHead(content, options),
		tail: truncate.truncateTail(content, options),
		line: truncate.truncateLine(content, Number(request.maxChars)),
		sanitized: shell.sanitizeBinaryOutput(content),
		...(request.formatBytes === undefined ? {} : { formattedSize: truncate.formatSize(Number(request.formatBytes)) }),
	};
	process.stdout.write(`${JSON.stringify({ wire: JSON.stringify(output) })}\n`);
}
}
void main().catch((error: unknown) => { console.error(error); process.exitCode = 1; });
