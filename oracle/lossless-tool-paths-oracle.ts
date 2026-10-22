import { writeFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main() {
	const module = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin, terminal: false })) {
		const input = JSON.parse(line);
		if (input.kind === "display") {
			const raw = String.fromCharCode(...input.units);
			const output = module.normalizeDisplayText(module.replaceTabs(module.str(raw)));
			process.stdout.write(`${JSON.stringify({ units: Array.from({ length: output.length }, (_, i) => output.charCodeAt(i)) })}\n`);
			continue;
		}
		if (input.kind === "localWriteError") {
			try {
				await writeFile(input.path, "x", "utf8");
				process.stdout.write(`${JSON.stringify({ error: null })}\n`);
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				process.stdout.write(`${JSON.stringify({ error: error.message })}\n`);
			}
			continue;
		}
		const calls: unknown[] = [];
		if (input.kind === "edit") {
			const tool = module.createEditTool(input.cwd, {
				operations: {
					access: async (path: string) => { calls.push(["access",path]); },
					readFile: async (path: string) => { calls.push(["read",path]); return Buffer.from("😀"); },
					writeFile: async (path: string, content: string) => { calls.push(["write",path,content]); },
				},
			});
			const result = await tool.execute("raw-edit", input.params);
			process.stdout.write(`${JSON.stringify({calls,result})}\n`);
			continue;
		}
		if (input.kind === "read") {
			const tool = module.createReadTool(input.cwd, {
				operations: {
					access: async (path: string) => { calls.push(["access",path]); },
					readFile: async (path: string) => { calls.push(["read",path]); return Buffer.alloc(10241, "x"); },
				},
			});
			const result = await tool.execute("raw-read", input.params);
			process.stdout.write(`${JSON.stringify({calls,result})}\n`);
			continue;
		}
		if (input.kind === "ls") {
			const tool = module.createLsTool(input.cwd, {
				operations: {
					exists: async (path: string) => { calls.push(["exists", path]); return input.exists ?? true; },
					stat: async (path: string) => { calls.push(["isDirectory", path]); return { isDirectory: () => !path.endsWith("file-\udfff") }; },
					readdir: async (path: string) => { calls.push(["readdir", path]); return input.entries; },
				},
			});
			try {
				const result = await tool.execute("raw-ls", input.params);
				process.stdout.write(`${JSON.stringify({ calls, result })}\n`);
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				process.stdout.write(`${JSON.stringify({ calls, error: error.message })}\n`);
			}
			continue;
		}
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
