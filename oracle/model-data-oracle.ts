import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath, pathToFileURL } from "node:url";
import { format } from "node:util";
import vm from "node:vm";

const source = process.argv[2];

async function check(packageRoot: string, api: Record<string, unknown>) {
	let stdout = "";
	let stderr = "";
	const state = { exitCode: 0 };
	const context = vm.createContext({
		console: {
			log: (...args: unknown[]) => {
				stdout += `${format(...args)}\n`;
			},
			error: (...args: unknown[]) => {
				stderr += `${format(...args)}\n`;
			},
		},
		process: state,
		Error,
	});
	const module = new vm.SourceTextModule(
		readFileSync(join(dirname(source), "check-model-data.ts"), "utf8"),
		{
			context,
			initializeImportMeta(meta) {
				meta.url = pathToFileURL(
					join(packageRoot, "scripts", "check-model-data.ts"),
				).href;
			},
		},
	);
	await module.link((specifier) => {
		const exports =
			specifier === "./model-data.ts"
				? api
				: specifier === "node:path"
					? { dirname, join }
					: { fileURLToPath };
		return new vm.SyntheticModule(
			Object.keys(exports),
			function () {
				for (const [name, value] of Object.entries(exports))
					this.setExport(name, value);
			},
			{ context },
		);
	});
	await module.evaluate();
	return { code: state.exitCode, stdout, stderr };
}

function writeResponse(response: unknown, encodeResponse: boolean) {
	const json = JSON.stringify(response);
	process.stdout.write(`${encodeResponse ? JSON.stringify(json) : json}\n`);
}

async function main() {
	const api = await import(pathToFileURL(source).href);
	for await (const line of createInterface({ input: process.stdin })) {
		let encodeResponse = false;
		try {
			const request = JSON.parse(line);
			encodeResponse = request.encodeResponse === true;
			let output;
			switch (request.op) {
				case "ids":
					output = api.assertExactModelIds(
						request.label,
						request.expected,
						request.actual,
					);
					break;
				case "hash":
					output = api.modelDataStructureHash(request.structure);
					break;
				case "manifest":
					output = api.createModelDataManifest(
						request.structure,
						request.files,
						request.generatedAt,
					);
					break;
				case "providers":
					output = api.readModelDataProviderIds(request.root);
					break;
				case "structure":
					output = api.readModelDataStructure(request.root);
					break;
				case "directory":
					output = api.validateModelDataDirectory(
						request.structure,
						request.dir,
					);
					break;
				case "generated":
					output = api.validateGeneratedModelData(request.root);
					break;
				case "check":
					output = await check(request.root, api);
					break;
				case "watchdog_probe":
					output = {
						pid: process.pid,
						callerFlag: typeof vm.SourceTextModule === "function",
					};
					break;
				default:
					throw new Error(`Unknown operation: ${request.op}`);
			}
			writeResponse({ ok: true, output: output ?? null }, encodeResponse);
		} catch (error) {
			writeResponse(
				{
					ok: false,
					error: error instanceof Error ? error.message : String(error),
				},
				encodeResponse,
			);
		}
	}
}
void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
