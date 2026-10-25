import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(
	pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href
);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const { text, command } = JSON.parse(line) as {
			text: string;
			command: string;
		};
		const path = InteractiveMode.prototype.getPathCommandArgument.call(
			{},
			text,
			command,
		);
		console.log(
			JSON.stringify({
				ok: true,
				defined: path !== undefined,
				path: path ?? null,
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
