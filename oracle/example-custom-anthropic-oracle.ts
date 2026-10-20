// Evaluate the byte-pinned example with unchanged function bodies; imports are
// intercepted before evaluation, so neither the public SDK nor bundled hooks load.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { webcrypto } from "node:crypto";
import vm from "node:vm";
const sourcePath = process.argv[2];
const root = resolve(dirname(sourcePath), "../../../../..");
const require = createRequire(resolve(root, "package.json"));
const ts = require("typescript");
function compile(text: string) {
 return ts.transpileModule(text, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true } }).outputText;
}
const modelText = readFileSync(resolve(root, "packages/ai/src/models.ts"), "utf8");
const modelAst = ts.createSourceFile("models.ts", modelText, ts.ScriptTarget.Latest, true);
const costNode = modelAst.statements.find((n: any) => ts.isFunctionDeclaration(n) && n.name?.text === "calculateCost");
if (!costNode) throw Error("Pinned calculateCost declaration missing");
const costExports: any = {};
vm.runInNewContext(compile(costNode.getText(modelAst)), { exports: costExports });
const sourceBytes = readFileSync(sourcePath, "utf8");
async function execute(input: any) {
 const events: any[] = [], calls: any[] = [];
 const controller = new AbortController();
 let config: any;
 const sdk = class {
  messages = { stream: (params: any, options: any) => {
   calls.push({ type: "stream", params: structuredClone(params), signal: !!options.signal });
   if (input.setupError) throw Error(input.setupError);
   return (async function* () {
    for (const event of input.events ?? []) yield event;
    if (input.abort) controller.abort();
    if (input.streamError !== undefined) throw (input.thrownValue ? input.streamError : Error(input.streamError));
   })();
  }};
  constructor(options: any) { calls.push({ type: "client", options }); }
 };
 let finish: () => void;
 const finished = new Promise<void>(r => finish = r);
 const sandbox: any = {
  exports: {}, URLSearchParams, TextEncoder, atob, btoa, AbortController, Error,
  Date: { now: () => input.now ?? 1700000000000 },
  crypto: { getRandomValues: (a: Uint8Array) => { a.set(input.random ?? Array.from({length:32}, (_,i)=>i)); return a; }, subtle: webcrypto.subtle },
  fetch: async (url: string, init: any) => {
   calls.push({ type: "fetch", url, method: init.method, headers: init.headers, body: JSON.parse(init.body), signal: !!init.signal });
   if (input.fetchError) throw Error(input.fetchError);
   return { ok: (input.status ?? 200) < 300, text: async () => input.body ?? "", json: async () => input.token ?? { access_token: "FAKE-access", refresh_token: "FAKE-refresh", expires_in: 3600 } };
  },
  require: (name: string) => {
   if (name === "@anthropic-ai/sdk") return input.wire ? require(name) : { __esModule: true, default: sdk };
   if (name === "@earendil-works/pi-ai") return {
    calculateCost: costExports.calculateCost,
    createAssistantMessageEventStream: () => ({ push: (e: any) => events.push(structuredClone(e)), end: () => finish() }),
   };
   throw Error(`Uncontrolled import ${name}`);
  },
 };
 vm.runInNewContext(compile(sourceBytes), sandbox, { filename: sourcePath });
 sandbox.exports.default({ registerProvider: (id: string, value: any) => { config = { id, ...value }; } });
 const metadata = { id: config.id, baseUrl: config.baseUrl, apiKey: config.apiKey, api: config.api, models: config.models, oauthName: config.oauth.name };
 if (input.kind === "providerAuth") {
  const resolverExports: any = {};
  const forbidden = () => { throw Error("No shell/default-registry work permitted"); };
  vm.runInNewContext(compile(readFileSync(resolve(root,"packages/coding-agent/src/core/resolve-config-value.ts"),"utf8")), {
   exports:resolverExports, process:{env:{}}, require:(name: string) => name === "child_process" ? {execSync:forbidden,spawnSync:forbidden} : name === "../utils/shell.ts" ? {getShellConfig:forbidden} : forbidden(), Error,
  });
  const composerExports: any = {};
  vm.runInNewContext(compile(readFileSync(resolve(root,"packages/coding-agent/src/core/provider-composer.ts"),"utf8")), {
   exports:composerExports, require:(name: string) => name === "./resolve-config-value.ts" ? resolverExports : name === "@earendil-works/pi-ai" ? {lazyStream:forbidden} : name === "@earendil-works/pi-ai/compat" ? {getApiProvider:forbidden} : forbidden(), Error,
  });
  const provider=composerExports.composeModelProvider(config.id,undefined,{getProvider:()=>undefined},config);
  const ctx={env:async (name: string)=>input.env?.[name]};
  const argument={ctx,credential:input.credential,signal:controller.signal};
  let result: any; try { result=await provider.auth.apiKey[input.check ? "check" : "resolve"](argument); } catch(e) { return {name:provider.name,error:(e as Error).message}; }
  return {name:provider.name,result:result ?? null};
 }
 if (input.kind === "metadata") return { metadata };
 if (input.kind === "login" || input.kind === "refresh") {
  try {
   const credential = input.kind === "login" ? await config.oauth.login({ onAuth: (value: any) => calls.push({type:"auth", ...value}), onPrompt: async (value: any) => { calls.push({type:"prompt", ...value}); if (input.promptError) throw Error(input.promptError); return input.code ?? "FAKE-code#FAKE-state#ignored"; } }) : await config.oauth.refreshToken(input.credential, controller.signal);
   return { calls, credential, auth: config.oauth.getApiKey(credential) };
  } catch (e) { return { calls, error: e instanceof Error ? e.message : String(e) }; }
 }
 const model = { ...config.models[input.modelIndex ?? 0], api: config.api, provider: config.id, baseUrl: config.baseUrl, ...input.model };
 const options = { ...input.options, signal: controller.signal };
 if (input.numericBudget) options.thinkingBudgets = { high: Number(input.numericBudget) };
 config.streamSimple(model, input.context ?? { messages: [] }, options);
 await finished;
 return { calls, events, ...(input.wire ? { sdkVersion: JSON.parse(readFileSync(resolve(root, "node_modules/@anthropic-ai/sdk/package.json"), "utf8")).version } : {}) };
}
async function main() {
 process.stdin.setEncoding("utf8"); let pending = "";
 for await (const chunk of process.stdin) {
  pending += chunk; let end: number;
  while ((end = pending.indexOf("\n")) !== -1) {
   const input = JSON.parse(pending.slice(0,end)); pending = pending.slice(end+1);
   console.log(JSON.stringify(await execute(input)));
  }
 }
 if (pending.length) throw Error("Unterminated request");
}
main().catch(error => { console.error(error); process.exit(1); });
