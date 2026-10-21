import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main() {
    const { parseStreamingJson } = await import(pathToFileURL(process.argv[2]).href);
    for await (const line of createInterface({ input: process.stdin })) {
        const request: { input?: string; inputUnits?: number[] } = JSON.parse(line);
        const input = request.inputUnits ? String.fromCharCode(...request.inputUnits) : request.input;
        const value = parseStreamingJson(input);
        console.log(JSON.stringify({ wire: JSON.stringify(value) }));
    }
}
main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});
