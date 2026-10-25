import { readFileSync } from "node:fs";
import { createInterface } from "node:readline";
import vm from "node:vm";
import { createRequire } from "node:module";

type Probe = (entries: unknown[]) => unknown;
const source = process.argv[2];
const ts = createRequire(source)("typescript");
const helper = source.endsWith("handoff.ts") ? "getHandoffMessages" : "buildConversationText";
const context = vm.createContext({
    exports: {} as { probe?: Probe },
    require(name: string) {
        if (name === "@earendil-works/pi-ai/compat" || name === "@earendil-works/pi-coding-agent" || name === "@earendil-works/pi-tui") return {};
        throw new Error(`Unexpected import ${name}`);
    },
});
const javascript = ts.transpileModule(readFileSync(source, "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
// Expose the unchanged module-local pure helper; never invoke the extension factory.
vm.runInContext(`${javascript}\nexports.probe = ${helper};`, context);
const probe: Probe = context.exports.probe;
async function main() {
    for await (const line of createInterface({ input: process.stdin })) {
        const request: { entries: unknown[] } = JSON.parse(line);
        process.stdout.write(`${JSON.stringify(probe(request.entries))}\n`);
    }
}
void main();
