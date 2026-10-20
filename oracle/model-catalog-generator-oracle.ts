import * as fs from "node:fs";
import { readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createRequire } from "node:module";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import vm from "node:vm";

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
const transformer = (context: any) => (root: any) => {
	const visit = (node: any): any => {
		if (
			ts.isExpressionStatement(node) &&
			ts.isCallExpression(node.expression) &&
			ts.isPropertyAccessExpression(node.expression.expression) &&
			ts.isCallExpression(node.expression.expression.expression) &&
			node.expression.expression.expression.expression.getText(root) ===
				"generateModels"
		)
			return undefined;
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
const output = ts.transpileModule(
	`${source}\n;globalThis.__api = {${names.join(",")}};`,
	{
		fileName: sourcePath,
		compilerOptions: {
			target: ts.ScriptTarget.ES2022,
			module: ts.ModuleKind.CommonJS,
		},
		transformers: { before: [transformer] },
	},
).outputText;
async function main(): Promise<void> {
	const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of lines) {
		const logs: unknown[] = [];
		const requests: string[] = [];
		const diagnostics: string[] = [];
		let root: string | undefined;
		try {
			const request = JSON.parse(line);
			if (request.operation === "generateModels") {
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
							if (arg?.stack) diagnostics.push(String(arg.stack));
					},
				},
				fetch: async (url: string) => {
					requests.push(url);
					const fixture = request.http?.[url];
					if (!fixture) throw new Error(`Unmocked request: ${url}`);
					if (fixture.error) throw new Error(fixture.error);
					return {
						ok: (fixture.status ?? 200) >= 200 && (fixture.status ?? 200) < 300,
						status: fixture.status ?? 200,
						json: async () => JSON.parse(fixture.body),
					};
				},
			});
			vm.runInContext(output, context, { filename: sourcePath });
			const api = context.__api as Record<
				string,
				(...args: unknown[]) => unknown
			>;
			if (!names.includes(request.operation))
				throw new Error(`Unknown operation: ${request.operation}`);
			const args = request.argsJson
				? JSON.parse(request.argsJson)
				: (request.args ?? []);
			const result = await api[request.operation](...args);
			process.stdout.write(
				`${JSON.stringify({ json: JSON.stringify(request.mutated ? args[0] : result), files: root ? collectFiles(root) : undefined, logs, requests, diagnostics, root })}\n`,
			);
		} catch (error: any) {
			process.stdout.write(
				`${JSON.stringify({ error: { name: error.name, message: error.message }, logs, requests, diagnostics: [...diagnostics, String(error.stack)] })}\n`,
			);
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
