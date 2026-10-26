import * as fs from "node:fs";
import { createHash } from "node:crypto";
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
const baseline = fs.readFileSync(path, "utf8");
const lock = JSON.parse(fs.readFileSync(new URL("../docs/porting/delta-lock.json", import.meta.url), "utf8"));
const delta = lock.deltas.find((entry: { commit: string }) => entry.commit === "5a0ea4d393e21d190524f26f1375486d7b86f7cb");
if (!delta) throw new Error("Terra delta missing from lock");
const relative = "packages/ai/scripts/generate-models.ts";
const input = fs.readFileSync(join(delta.checkout, relative), "utf8");
if (createHash("sha256").update(input).digest("hex") !== delta.modules[relative]) throw new Error("Terra generator lock mismatch");
const terra = `		// https://developers.openai.com/api/docs/models/gpt-6.1-terra
		{
			id: "gpt-6.1-terra",
			name: "GPT-6.1 Terra",
			api: "openai-codex-responses",
			provider: "openai-codex",
			baseUrl: CODEX_BASE_URL,
			reasoning: true,
			thinkingLevelMap: { max: "max" },
			input: ["text", "image"],
			cost: { input: 1, output: 5, cacheRead: 0.05, cacheWrite: 1.25 },
			contextWindow: 922000,
			maxTokens: CODEX_MAX_TOKENS,
		},
`;
if (!input.includes(terra) || input.replace(terra, "") !== baseline) throw new Error("Unexpected Terra generator delta");
const cloudflare = await import(
	pathToFileURL(join(dirname(path), "../src/api/cloudflare.ts")).href
);
const transformer: import("typescript").TransformerFactory<
	import("typescript").SourceFile
> = (context) => (root) => {
	const visit: import("typescript").Visitor = (node) => {
		if (
			ts.isVariableStatement(node) &&
			node.declarationList.declarations.some(
				(declaration) => declaration.name.getText(root) === "generatedHeader",
			)
		) {
			const capture = ts.factory.createExpressionStatement(
				ts.factory.createAssignment(
					ts.factory.createPropertyAccessExpression(
						ts.factory.createIdentifier("globalThis"),
						"__catalog",
					),
					ts.factory.createIdentifier("providers"),
				),
			);
			return [capture, node];
		}
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
function snapshot(value: unknown): unknown {
	if (Object.prototype.toString.call(value) === "[object Error]") {
		const error = value as Error & Record<string, unknown>;
		return {
			name: error.name,
			message: error.message,
			...Object.fromEntries(
				Object.keys(error)
					.filter((key) => key !== "stack")
					.map((key) => [key, snapshot(error[key])]),
			),
		};
	}
	return value;
}
type FileEvent = ["remove", string] | ["write", string, string];

function evaluateProviderModule(filename: string, contents: string): unknown {
	const compiled = ts.transpileModule(contents, {
		fileName: filename,
		reportDiagnostics: true,
		compilerOptions: {
			target: ts.ScriptTarget.ES2022,
			module: ts.ModuleKind.CommonJS,
		},
	});
	if (compiled.diagnostics?.length) {
		return {
			diagnostics: compiled.diagnostics.map((entry) => ({
				code: entry.code,
				message: ts.flattenDiagnosticMessageText(entry.messageText, "\n"),
			})),
		};
	}
	const module = vm.createContext({ exports: {} });
	vm.runInContext(compiled.outputText, module, { timeout: 500 });
	return Object.values(module.exports)[0];
}

function loadEmittedProviders(
	files: FileEvent[],
	filesystemRoot: string | undefined,
): Record<string, unknown> {
	const loaded: Record<string, unknown> = {};
	for (const event of files) {
		if (event[0] !== "write" || !event[1].endsWith(".models.ts")) continue;
		const contents = filesystemRoot
			? fs.readFileSync(event[1], "utf8")
			: Buffer.from(event[2], "utf8").toString("utf8");
		loaded[event[1]] = evaluateProviderModule(event[1], contents);
	}
	return loaded;
}

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const logs: unknown[] = [];
	const requests: string[] = [];
	const files: FileEvent[] = [];
	let writes = 0;
	const filesystem = {
		readdirSync: (directory: string) =>
			request.filesystemRoot
				? fs.readdirSync(directory)
				: (request.files ?? []),
		rmSync: (path: string) => {
			if (request.filesystemRoot) fs.rmSync(path);
			files.push(["remove", path]);
		},
		writeFileSync: (path: string, contents: string) => {
			writes += 1;
			if (request.filesystemRoot) {
				if (request.failWrite === writes) fs.mkdirSync(path);
				fs.writeFileSync(path, contents);
			}
			files.push(["write", path, contents]);
		},
	};
	const context = vm.createContext({
		__moduleUrl: pathToFileURL(
			request.filesystemRoot
				? join(request.filesystemRoot, "scripts/generate-models.ts")
				: path,
		).href,
		exports: {},
		fetch: async (url: string) => {
			requests.push(url);
			const response = request.responses[url];
			if (!response) throw new Error(`Unexpected fetch ${url}`);
			if ("throw" in response) throw response.throw;
			if (response.error) throw new Error(response.error);
			return {
				status: response.status ?? 200,
				ok: (response.status ?? 200) < 400,
				json: async () =>
					response.bodyText === undefined
						? response.body
						: JSON.parse(response.bodyText),
			};
		},
		require: (id: string) =>
			id === "fs"
				? filesystem
				: id === "../src/api/cloudflare.ts"
					? cloudflare
					: require(id),
		console: {
			log: (...args: unknown[]) => logs.push(["log", ...args.map(snapshot)]),
			error: (...args: unknown[]) =>
				logs.push(["error", ...args.map(snapshot)]),
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
		} else if (
			request.operation === "workflow" ||
			request.operation === "emission"
		) {
			await api.generateModels();
			const catalog = Object.fromEntries(
				Object.keys(context.__catalog)
					.sort()
					.map((provider) => [
						provider,
						Object.fromEntries(
							Object.keys(context.__catalog[provider])
								.sort()
								.map((id) => [id, context.__catalog[provider][id]]),
						),
					]),
			);
			const loaded =
				request.operation === "emission"
					? loadEmittedProviders(files, request.filesystemRoot)
					: {};
			process.stdout.write(
				`${JSON.stringify({ ok: true, catalog, loaded, logs, requests, files })}\n`,
			);
		} else if (request.operation === "normalizeNvidiaModelId") {
			const result = api.normalizeNvidiaModelId(
				...JSON.parse(request.argsJson),
			);
			process.stdout.write(
				`${JSON.stringify({ ok: true, json: JSON.stringify(result) })}\n`,
			);
		} else if (
			[
				"loadModelsDevData",
				"fetchOpenRouterModels",
				"fetchAiGatewayModels",
				"fetchNvidiaNimModelIds",
			].includes(request.operation)
		) {
			const result = await api[request.operation]();
			process.stdout.write(
				`${JSON.stringify({ ok: true, result: request.operation === "fetchNvidiaNimModelIds" ? [...result] : result, logs, requests })}\n`,
			);
		} else {
			throw new Error(
				`Unknown generator probe operation: ${request.operation}`,
			);
		}
	} catch (error) {
		process.stdout.write(
			`${JSON.stringify({ ok: false, error: snapshot(error), logs, requests, files })}\n`,
		);
	}
}
