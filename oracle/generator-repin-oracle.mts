import * as fs from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import vm from "node:vm";

const path = process.argv[2];
const require = createRequire(path);
const ts = require("typescript") as typeof import("typescript");
const names = [
	"detectOpenAICompletionsCompat",
	"applyOpenAICompletionsCompatMetadata",
	"applyThinkingLevelMetadata",
	"getTogetherCompat",
	"getTogetherThinkingLevelMap",
	"getAnthropicMessagesCompat",
	"getBedrockBaseUrl",
	"normalizeNvidiaModelId",
	"roundCost",
	"fetchNvidiaNimModelIds",
	"fetchOpenRouterModels",
	"fetchAiGatewayModels",
	"loadModelsDevData",
	"generateModels",
];
const input = fs.readFileSync(path, "utf8");
const cloudflare = await import(
	pathToFileURL(join(dirname(path), "../src/api/cloudflare.ts")).href
);
const transformer: import("typescript").TransformerFactory<
	import("typescript").SourceFile
> = (context) => (root) => {
	const visit: import("typescript").Visitor = (node) => {
		if (
			ts.isExpressionStatement(node) &&
			ts.isCallExpression(node.expression) &&
			ts.isPropertyAccessExpression(node.expression.expression) &&
			node.expression.expression.expression.getText(root) === "generateModels()"
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
	return ts.visitNode(root, visit) as import("typescript").SourceFile;
};
const output = ts.transpileModule(
	`${input}\nglobalThis.__api={${names.join(",")}}`,
	{
		fileName: path,
		compilerOptions: {
			target: ts.ScriptTarget.ES2022,
			module: ts.ModuleKind.CommonJS,
		},
		transformers: { before: [transformer] },
	},
).outputText;
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const logs: unknown[] = [];
	const context = vm.createContext({
		__moduleUrl: pathToFileURL(path).href,
		exports: {},
		require: (id: string) =>
			id === "../src/api/cloudflare.ts" ? cloudflare : require(id),
		console: {
			log: (...args: unknown[]) => logs.push(["log", ...args]),
			error: (...args: unknown[]) => logs.push(["error", ...args]),
		},
	});
	try {
		vm.runInContext(output, context);
		const api = context.__api;
		if (request.operation === "metadata") {
			const results = request.models.map((model: unknown) => {
				api.applyThinkingLevelMetadata(model);
				api.applyOpenAICompletionsCompatMetadata(model);
				return model;
			});
			process.stdout.write(
				`${JSON.stringify({ ok: true, result: results })}\n`,
			);
		} else {
			throw new Error(
				`Unknown generator probe operation: ${request.operation}`,
			);
		}
	} catch (error) {
		const candidate = error as { name?: unknown; message?: unknown };
		process.stdout.write(
			`${JSON.stringify({ ok: false, error: { name: candidate?.name, message: candidate?.message } })}\n`,
		);
	}
}
