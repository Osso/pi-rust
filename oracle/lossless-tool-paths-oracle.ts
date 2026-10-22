import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main() {
	const module = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin, terminal: false })) {
		const input = JSON.parse(line);
		const calls: unknown[] = [];
		const tool = module.createWriteTool(input.cwd, {
			operations: {
				mkdir: async (path: string) => { calls.push(["mkdir", path]); },
				writeFile: async (path: string, content: string) => { calls.push(["write", path, content]); },
			},
		});
		const result = await tool.execute("raw-path", input.params);
		process.stdout.write(`${JSON.stringify({ calls, text: result.content[0].text })}\n`);
	}
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
