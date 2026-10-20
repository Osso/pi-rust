import { readFileSync } from "node:fs";
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
const names = ["readGeneratorOptions", "getModelsDevCost", "withOpenAiLongContextPricing", "processBasetenModels", "processFireworksModels", "fetchOpenRouterModels", "fetchAiGatewayModels", "fetchNvidiaNimModelIds", "loadModelsDevData", "generateModels"];
const transformer = (context: any) => (root: any) => {
 const visit = (node: any): any => {
  if (ts.isExpressionStatement(node) && ts.isCallExpression(node.expression) && ts.isPropertyAccessExpression(node.expression.expression) && ts.isCallExpression(node.expression.expression.expression) && node.expression.expression.expression.expression.getText(root) === "generateModels") return undefined;
  if (ts.isMetaProperty(node) && node.keywordToken === ts.SyntaxKind.ImportKeyword) return ts.factory.createObjectLiteralExpression([ts.factory.createPropertyAssignment("url", ts.factory.createStringLiteral(pathToFileURL(sourcePath).href))]);
  return ts.visitEachChild(node, visit, context);
 };
 return ts.visitNode(root, visit);
};
const output = ts.transpileModule(`${source}\n;globalThis.__api = {${names.join(",")}};`, { fileName: sourcePath, compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS }, transformers: { before: [transformer] } }).outputText;
async function main(): Promise<void> {
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const line of lines) {
 try {
  const request = JSON.parse(line);
  const logs: unknown[] = [];
  const requests: string[] = [];
  const context = vm.createContext({ require: requireSource, exports: {}, process: { argv: ["node", sourcePath] }, console: { log: (...args: unknown[]) => logs.push(["log", ...args]), error: (...args: unknown[]) => logs.push(["error", ...args.map(String)]) }, fetch: async (url: string) => {
   requests.push(url);
   const fixture = request.http?.[url];
   if (!fixture) throw new Error(`Unmocked request: ${url}`);
   if (fixture.error) throw new Error(fixture.error);
   return { ok: (fixture.status ?? 200) >= 200 && (fixture.status ?? 200) < 300, status: fixture.status ?? 200, json: async () => JSON.parse(fixture.body) };
  } });
  vm.runInContext(output, context, { filename: sourcePath });
  const api = context.__api as Record<string, (...args: unknown[]) => unknown>;
  if (!names.includes(request.operation)) throw new Error(`Unknown operation: ${request.operation}`);
  const result = await api[request.operation](...(request.args ?? []));
  process.stdout.write(`${JSON.stringify({ json: JSON.stringify(result), logs, requests })}\n`);
 } catch (error: any) {
  process.stdout.write(`${JSON.stringify({ error: { name: error.name, message: error.message } })}\n`);
 }
}
}
void main().catch((error) => { process.stderr.write(String(error)); process.exitCode = 1; });
