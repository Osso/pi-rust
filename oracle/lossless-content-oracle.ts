import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
async function main() {
    const { sanitizeSurrogates } = await import(pathToFileURL(process.argv[2]).href);
    for await (const line of createInterface({ input: process.stdin })) {
        const request = JSON.parse(line);
        const value: { content?: Array<{ type: string; text: string }> } = JSON.parse(request.wire);
        const text = value.content?.filter((part) => part.type === "text").map((part) => part.text).join("\n");
        console.log(JSON.stringify({
            wire: JSON.stringify(value),
            units: text === undefined ? null : units(text),
            sanitized: text === undefined ? null : sanitizeSurrogates(text),
        }));
    }
}
main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});
