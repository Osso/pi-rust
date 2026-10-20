import { readFileSync } from "node:fs";
import vm from "node:vm";
import { format } from "node:util";

// Execute the pinned program unchanged. Only its SDK import is linked to a
// controlled factory; no package barrel, configuration, or provider is loaded.
const source = readFileSync(process.argv[2], "utf8");
async function run(request: any) {
	const events: any[] = [];
	const messages = request.messages;
	let stdout = "";
	let observer: (event: any) => void;
	const session = {
		subscribe(listener: (event: any) => void) { events.push(["subscribe"]); observer = listener; },
		async prompt(...args: any[]) {
			events.push(["prompt", args]);
			for (const event of request.events) observer(event);
			if (request.failure) throw new Error(request.failure);
		},
		state: { messages },
		dispose() { events.push(["dispose"]); },
	};
	const context = vm.createContext({
		process: { stdout: { write(text: string) { events.push(["write", text]); stdout += text; } } },
		console: { log(...args: any[]) { events.push(["log", args]); stdout += format(...args) + "\n"; } },
	});
	const importStatement = 'import { createAgentSession } from "@earendil-works/pi-coding-agent";';
	if (source.split(importStatement).length !== 2) throw new Error("Expected the pinned SDK import seam");
	// Remove only the import; execute the complete remaining body verbatim.
	const program = vm.compileFunction(`return (async () => {\n${source.replace(importStatement, "")}\n})();`, ["createAgentSession"], { parsingContext: context });
	let error: string | null = null;
	let errorName: string | null = null;
	try {
		await program(async (...args: any[]) => {
			events.push(["createAgentSession", args]);
			return { session };
		});
	} catch (failure) {
		error = (failure as Error).message;
		errorName = (failure as Error).name;
	}
	return { events, error, errorName, stdout };
}
async function main() {
	let pending = "";
	process.stdin.setEncoding("utf8");
	for await (const chunk of process.stdin) {
		pending += chunk;
		let end: number;
		while ((end = pending.indexOf("\n")) !== -1) {
			const line = pending.slice(0, end);
			pending = pending.slice(end + 1);
			console.log(JSON.stringify(await run(JSON.parse(line))));
		}
	}
	if (pending.length) throw new Error("Unterminated oracle request");
}
main().catch((error) => { console.error(error); process.exit(1); });
