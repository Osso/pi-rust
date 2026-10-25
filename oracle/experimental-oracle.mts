import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source)
	throw new Error("experimental oracle requires the frozen module path");
const api = (await import(pathToFileURL(source).href)) as {
	areExperimentalFeaturesEnabled(): boolean;
};
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { id: number };
	process.stdout.write(
		`${JSON.stringify({ id: request.id, ok: true, output: api.areExperimentalFeaturesEnabled() })}\n`,
	);
}
