// Execute the immutable worker's matching functions, without its process entrypoint.
const fs = require("node:fs");
const path = require("node:path");
const readline = require("node:readline");
const vm = require("node:vm");

function loadWorker(fallback) {
    const source = fs.readFileSync(process.argv[2], "utf8");
    const entrypoint = source.lastIndexOf("\nmain()\n");
    if (entrypoint < 0) throw new Error("frozen worker entrypoint not found");
    const workerPath = fallback ? { ...path, matchesGlob: undefined } : path;
    const context = vm.createContext({
        require: (name) => name === "node:path" ? workerPath : require(name),
        process: { argv: ["node", "worker", "fixture", "{}"], cwd: () => "/tmp" },
    });
    new vm.Script(source.slice(0, entrypoint) + "\nglobalThis.matchers = { matchesGlob, makeMatcher };", {
        filename: process.argv[2],
    }).runInContext(context);
    return context.matchers;
}

const active = loadWorker(false);
const fallback = loadWorker(true);
const fromUnits = (units) => String.fromCharCode(...units);
const toUnits = (text) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
function executeCase(test) {
    const pattern = fromUnits(test.pattern);
    const text = fromUnits(test.text);
    try {
        const worker = test.fallback ? fallback : active;
        const matched = test.kind === "glob"
            ? worker.matchesGlob(text, pattern)
            : worker.makeMatcher(pattern, test.kind === "literal", test.ignoreCase)(text);
        return { matched };
    } catch (error) {
        return { error: { name: error.name, message: toUnits(error.message) } };
    }
}

readline.createInterface({ input: process.stdin }).on("line", (line) => {
    const request = JSON.parse(line);
    const output = request.cases.map(executeCase);
    process.stdout.write(JSON.stringify({ id: request.id, ok: true, output, node: process.version }) + "\n");
});
