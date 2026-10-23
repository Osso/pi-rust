import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("autocomplete source required");
const { CombinedAutocompleteProvider } = await import(pathToFileURL(source).href);

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { base: string; query: string };
	delete process.env.HOME;
	process.env.PI_TEST_FAIL_PASSWD = "1";
	const provider = new CombinedAutocompleteProvider([], request.base, "/nss-never-exec");
	const result = await provider.getSuggestions([request.query], 0, request.query.length, {
		forceFileSuggestions: true,
		signal: new AbortController().signal,
	});
	console.log(JSON.stringify(result));
}
