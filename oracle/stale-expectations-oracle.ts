import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
function taggedNumber(value: number): unknown {
	if (Number.isNaN(value)) return { kind: "nan" };
	return { kind: "number", value };
}
async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("missing frozen module");
	const source = pathToFileURL(modulePath);
	const { createCustomMessage } = await import(source.href);
	const { NodeExecutionEnv } = await import(
		new URL("./env/nodejs.ts", source).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		if (input.kind === "timestamps") {
			const results = input.timestamps.map((timestamp: string) => ({
				input: timestamp,
				parsed: taggedNumber(Date.parse(timestamp)),
				custom: taggedNumber(
					createCustomMessage("note", "plain", false, undefined, timestamp)
						.timestamp,
				),
			}));
			console.log(JSON.stringify(results));
		} else {
			const results = [];
			for (const home of input.homes) {
				process.env.HOME = home;
				const env = new NodeExecutionEnv({ cwd: "/work" });
				results.push({
					home,
					resolved: await env.absolutePath("~/session.jsonl"),
				});
			}
			console.log(JSON.stringify(results));
		}
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
