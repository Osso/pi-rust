import * as fs from "node:fs";
import { readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createRequire } from "node:module";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import vm from "node:vm";
import { inspect } from "node:util";

// Instrument the actual pinned module, exposing its private functions and removing
// only the automatic CLI invocation. All function bodies execute unchanged.
const sourcePath = process.argv[2];
const requireSource = createRequire(sourcePath);
const ts = requireSource("typescript");
const source = readFileSync(sourcePath, "utf8");
const names = [
	"readGeneratorOptions",
	"getModelsDevCost",
	"withOpenAiLongContextPricing",
	"processBasetenModels",
	"processFireworksModels",
	"fetchOpenRouterModels",
	"fetchAiGatewayModels",
	"fetchNvidiaNimModelIds",
	"normalizeNvidiaModelId",
	"loadModelsDevData",
	"generateModels",
	"detectOpenAICompletionsCompat",
	"applyOpenAICompletionsCompatMetadata",
	"applyThinkingLevelMetadata",
	"applyStrictToolCompatMetadata",
	"applyOpenAIGrammarToolCompatMetadata",
	"applyOpenAIToolSearchMetadata",
	"applyOpenAIExplicitPromptCacheMetadata",
];
const transformer = (cli: boolean) => (context: any) => (root: any) => {
	const visit = (node: any): any => {
		if (
			ts.isExpressionStatement(node) &&
			ts.isCallExpression(node.expression) &&
			ts.isPropertyAccessExpression(node.expression.expression) &&
			ts.isCallExpression(node.expression.expression.expression) &&
			node.expression.expression.expression.expression.getText(root) ===
				"generateModels"
		)
			return cli
				? ts.factory.createExpressionStatement(
						ts.factory.createAssignment(
							ts.factory.createPropertyAccessExpression(
								ts.factory.createIdentifier("globalThis"),
								"__completion",
							),
							node.expression,
						),
					)
				: undefined;
		if (
			ts.isMetaProperty(node) &&
			node.keywordToken === ts.SyntaxKind.ImportKeyword
		)
			return ts.factory.createObjectLiteralExpression([
				ts.factory.createPropertyAssignment(
					"url",
					ts.factory.createIdentifier("__moduleUrl"),
				),
			]);
		return ts.visitEachChild(node, visit, context);
	};
	return ts.visitNode(root, visit);
};
function compile(cli: boolean): string {
	return ts.transpileModule(
		`${source}\n;globalThis.__api = {${names.join(",")}};`,
		{
			fileName: sourcePath,
			compilerOptions: {
				target: ts.ScriptTarget.ES2022,
				module: ts.ModuleKind.CommonJS,
			},
			transformers: { before: [transformer(cli)] },
		},
	).outputText;
}
const output = compile(false);
const cliOutput = compile(true);
function respond(response: Record<string, unknown>): void {
	const serialized = JSON.stringify(response);
	if (Array.isArray(response.diagnostics) && response.diagnostics.length) {
		const directory = join(tmpdir(), "pi-rust-model-generator-proof");
		fs.mkdirSync(directory, { recursive: true });
		fs.appendFileSync(
			join(directory, `ts-diagnostics-${process.pid}.jsonl`),
			`${serialized}\n`,
		);
	}
	process.stdout.write(`${serialized}\n`);
}
async function* requestLines(): AsyncGenerator<string> {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	for await (const chunk of process.stdin) {
		buffer += decoder.write(chunk);
		let index: number;
		while ((index = buffer.indexOf("\n")) !== -1) {
			yield buffer.slice(0, index);
			buffer = buffer.slice(index + 1);
		}
	}
	buffer += decoder.end();
	if (buffer.length) throw new Error("Unterminated generator oracle request");
}
async function main(): Promise<void> {
	const lines = requestLines();
	for await (const line of lines) {
		const logs: unknown[] = [];
		const requests: string[] = [];
		const diagnostics: string[] = [];
		let root: string | undefined;
		let cli = false;
		try {
			const request = JSON.parse(line);
			cli = request.operation === "cli";
			if (request.operation === "generateModels" || cli) {
				root = fs.mkdtempSync(join(tmpdir(), "pi-model-generator-oracle-"));
				fs.mkdirSync(join(root, "src/providers"), { recursive: true });
				fs.writeFileSync(join(root, "src/models.generated.ts"), "");
				for (const [path, content] of Object.entries(
					request.initialFiles ?? {},
				)) {
					const file = resolve(root, path);
					if (!file.startsWith(`${root}/`))
						throw new Error("Fixture path escapes sandbox");
					fs.mkdirSync(dirname(file), { recursive: true });
					fs.writeFileSync(file, String(content));
				}
			}
			const fixtureFs = { ...fs };
			let injected = false;
			if (root) {
				for (const operation of [
					"writeFileSync",
					"mkdirSync",
					"mkdtempSync",
					"rmSync",
					"renameSync",
				] as const) {
					(fixtureFs as any)[operation] = (...args: any[]) => {
						for (const path of args.slice(
							0,
							operation === "renameSync" ? 2 : 1,
						))
							if (!resolve(String(path)).startsWith(`${root}/`))
								throw new Error("Generator write escapes fixture sandbox");
						const path = String(args[0]);
						const data = join(root!, "src/providers/data");
						const aggregator = join(root!, "src/models.generated.ts");
						const staged = path.includes(".model-generation-");
						const publicDir = join(root!, "catalog");
						if (!injected && request.fault === "public-remove" && operation === "rmSync" && path === publicDir) {
							injected = true;
							return fs.readFileSync(path, "utf8");
						}
						if (!injected && request.fault === "public-mkdir" && operation === "mkdirSync" && path === join(publicDir, "providers")) {
							injected = true;
							fs.writeFileSync(path, "blocking file");
						}
						const publicWrites: Record<string, string> = {
							"public-write-models": join(publicDir, "models.json"),
							"public-write-providers": join(publicDir, "providers.json"),
							"public-write-provider": join(publicDir, "providers/openai.json"),
						};
						if (!injected && operation === "writeFileSync" && path === publicWrites[request.fault]) {
							injected = true;
							fs.mkdirSync(path);
						}
						if (
							!injected &&
							operation === "writeFileSync" &&
							((request.fault === "stage-write" &&
								staged &&
								path.endsWith("/openai.json")) ||
								(request.fault === "include-write" && path === aggregator))
						) {
							injected = true;
							if (fs.existsSync(path)) fs.rmSync(path);
							fs.mkdirSync(path);
							try {
								return fs.writeFileSync(...(args as [any, any]));
							} finally {
								fs.rmSync(path, { recursive: true, force: true });
							}
						}
						if (
							!injected &&
							operation === "writeFileSync" &&
							request.fault === "stage-validate" &&
							staged &&
							path.endsWith("/openai.json")
						) {
							injected = true;
							return fs.writeFileSync(path, "{}");
						}
						if (
							!injected &&
							operation === "renameSync" &&
							String(args[1]) === data &&
							path.endsWith("/data")
						) {
							if (request.fault === "data-rename") {
								injected = true;
								fs.mkdirSync(data);
								fs.writeFileSync(join(data, "block"), "block");
								try {
									return fs.renameSync(path, data);
								} finally {
									fs.rmSync(data, { recursive: true, force: true });
								}
							}
							const result = fs.renameSync(path, data);
							if (request.fault === "post-manifest") {
								injected = true;
								const file = join(data, ".manifest.json");
								const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
								manifest.structureHash = "wrong";
								fs.writeFileSync(file, JSON.stringify(manifest));
							}
							if (request.fault === "post-provider") {
								injected = true;
								fs.writeFileSync(join(data, "openai.json"), "{}");
							}
							if (request.fault === "post-include") {
								injected = true;
								fs.writeFileSync(aggregator, "");
							}
							return result;
						}
						if (
							!injected &&
							operation === "rmSync" &&
							request.fault === "cleanup" &&
							staged &&
							!path.endsWith("/data")
						) {
							injected = true;
							return fs.readFileSync(path, "utf8");
						}
						return (fs[operation] as any)(...args);
					};
				}
			}
			const context = vm.createContext({
				__moduleUrl: pathToFileURL(
					root ? join(root, "scripts/generate-models.ts") : sourcePath,
				).href,
				Date: class extends Date {
					constructor(...args: any[]) {
						super(
							args.length
								? args[0]
								: (request.generatedAt ?? "2026-10-20T00:00:00.000Z"),
						);
					}
				},
				require: (name: string) =>
					name === "fs" ? fixtureFs : requireSource(name),
				exports: {},
				process: {
					argv: [
						"node",
						sourcePath,
						...(request.argv ?? []).map((arg: string) =>
							root && arg === "@OUTPUT@" ? join(root, "catalog") : arg,
						),
					],
				},
				console: {
					log: (...args: unknown[]) => logs.push(["log", ...args]),
					error: (...args: any[]) => {
						logs.push(["error", ...args.map(String)]);
						for (const arg of args)
							if (arg?.stack)
								diagnostics.push(inspect(arg, { depth: null, colors: false }));
					},
				},
				fetch: async (url: string) => {
					requests.push(url);
					const fixture = request.http?.[url];
					if (!fixture) throw new Error(`Unmocked request: ${url}`);
					if (fixture.error) throw new Error(fixture.error);
					if (fixture.nativeUrl) {
						const target = new URL(fixture.nativeUrl);
						if (target.hostname !== "127.0.0.1" || target.protocol !== "http:")
							throw new Error("Native fixture must use HTTP loopback");
						return fetch(target);
					}
					return {
						ok: (fixture.status ?? 200) >= 200 && (fixture.status ?? 200) < 300,
						status: fixture.status ?? 200,
						json: async () => JSON.parse(fixture.body),
					};
				},
			});
			vm.runInContext(cli ? cliOutput : output, context, {
				filename: sourcePath,
			});
			const api = context.__api as Record<
				string,
				(...args: unknown[]) => unknown
			>;
			if (!names.includes(request.operation) && !cli)
				throw new Error(`Unknown operation: ${request.operation}`);
			const args = request.argsJson
				? JSON.parse(request.argsJson)
				: (request.args ?? []);
			const result = cli
				? await context.__completion
				: await api[request.operation](...args);
			respond({
				json: JSON.stringify(
					request.mapEntries
						? Array.from(result as Iterable<unknown>)
						: request.mutated
							? args[0]
							: result,
				),
				files: root ? collectFiles(root) : undefined,
				logs,
				requests,
				diagnostics,
				root,
				exitCode: cli ? (context.process.exitCode ?? 0) : undefined,
			});
		} catch (error: any) {
			respond({
				error: {
					name: error.name,
					message: error.message,
					code: error.code,
					errno: error.errno,
					syscall: error.syscall,
					path: error.path,
					dest: error.dest,
				},
				files: root ? collectFiles(root) : undefined,
				root,
				logs,
				requests,
				diagnostics: [
					...diagnostics,
					inspect(error, { depth: null, colors: false }),
				],
				exitCode: cli ? 1 : undefined,
			});
		} finally {
			if (root) fs.rmSync(root, { recursive: true, force: true });
		}
	}
}
function collectFiles(root: string): Record<string, string> {
	const files: Record<string, string> = {};
	const visit = (directory: string) => {
		for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
			const path = join(directory, entry.name);
			if (entry.isDirectory()) visit(path);
			else files[path.slice(root.length + 1)] = fs.readFileSync(path, "utf8");
		}
	};
	visit(root);
	return files;
}
void main().catch((error) => {
	process.stderr.write(String(error));
	process.exitCode = 1;
});
