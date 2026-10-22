import { writeFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main() {
	const module = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin, terminal: false })) {
		const input = JSON.parse(line);
		if (input.kind === "render") {
			const terminal=await import(new URL("../../../../tui/src/terminal-image.ts",pathToFileURL(process.argv[2])).href);
			terminal.setCapabilities({...terminal.getCapabilities(),hyperlinks:true});
			const themes=await import(new URL("../../modes/interactive/theme/theme.ts",pathToFileURL(process.argv[2])).href);
			const theme=themes.loadThemeFromPath(input.themePath,"truecolor");
			themes.setThemeInstance(theme);
			const definition=module[input.factory](input.cwd);
			const context={args:input.params,toolCallId:"raw-render",invalidate:()=>{},lastComponent:undefined,callComponent:undefined,state:{},cwd:input.cwd,executionStarted:false,argsComplete:false,isPartial:false,expanded:true,showImages:false,isError:true};
			const call=definition.renderCall(input.params,theme,context);
			const result=definition.renderResult(input.result,{expanded:true,isPartial:false},theme,context);
			process.stdout.write(`${JSON.stringify({call:call.render(input.width),result:result.render(input.width)})}\n`);
			continue;
		}
		if (input.kind === "bootstrapSplit") {
			try {module.spawnGatedDetachedPayload(input.input);throw new Error("expected split output rejection");}
			catch(error) {
				if (!(error instanceof Error)) throw error;
				process.stdout.write(`${JSON.stringify({error:error.message})}\n`);
			}
			continue;
		}
		if (input.kind === "detachedArtifacts") {
			try {
				const artifacts=module[input.operation](input.root,input.job);
				process.stdout.write(`${JSON.stringify({artifacts})}\n`);
			} catch(error) {
				if (!(error instanceof Error)) throw error;
				process.stdout.write(`${JSON.stringify({error:error.message})}\n`);
			}
			continue;
		}
		if (input.kind === "fileAttachment") {
			const result=await module.processFileArguments(input.paths);
			process.stdout.write(`${JSON.stringify(result)}\n`);
			continue;
		}
		if (input.kind === "shellMissing") {
			try { module.getShellConfig(input.path); throw new Error("expected missing shell"); }
			catch(error) {
				if (!(error instanceof Error)) throw error;
				process.stdout.write(`${JSON.stringify({error:error.message})}\n`);
			}
			continue;
		}
		if (input.kind === "cleanup") {
			const shell = await import(pathToFileURL(process.argv[2].replace(/ansi\.ts$/, "shell.ts")).href);
			const output = shell.sanitizeBinaryOutput(module.stripAnsi(String.fromCharCode(...input.units)));
			process.stdout.write(`${JSON.stringify({ units: Array.from({length:output.length},(_,i)=>output.charCodeAt(i)) })}\n`);
			continue;
		}
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
		if (input.kind === "grepMissing") {
			const paths: string[] = [];
			const tool = module.createGrepTool(input.cwd, {
				operations: {
					isDirectory: async (path: string) => { paths.push(path); throw new Error("missing"); },
					readFile: async () => { throw new Error("unexpected read"); },
				},
			});
			try {
				await tool.execute("raw-grep",input.params);
				throw new Error("expected missing path");
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				process.stdout.write(`${JSON.stringify({paths,error:error.message})}\n`);
			}
			continue;
		}
		if (input.kind === "find") {
			const tool = module.createFindTool(input.cwd, {
				operations: {
					exists: async (path: string) => { calls.push(["exists",path]); return true; },
					glob: async (pattern: string, cwd: string, options: {ignore:string[];limit:number}) => {
						calls.push(["glob",pattern,cwd,options.ignore,options.limit]);
						return [`${cwd}/file-\udfff`];
					},
				},
			});
			const result = await tool.execute("raw-find", input.params);
			process.stdout.write(`${JSON.stringify({calls,result})}\n`);
			continue;
		}
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
