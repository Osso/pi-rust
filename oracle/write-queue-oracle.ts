import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { setImmediate } from "node:timers/promises";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Tool = { execute(id: string, args: unknown, signal?: AbortSignal): Promise<unknown> };
type Api = { createWriteToolDefinition(cwd: string, options: unknown): Tool };
type Scenario = { root: string; phase: "mkdir" | "write"; abort: boolean; reject?: string; different?: boolean; path?: string; preabort?: boolean };

async function runScenario(api: Api, input: Scenario) {
    const events: unknown[] = [];
    const gate = Promise.withResolvers<void>();
    const started = Promise.withResolvers<void>();
    const controller = new AbortController();
    const record = (label: string, operation: string, state: string, path: string, content?: string) => {
        events.push({ label, operation, state, path: path.slice(input.root.length), ...(content === undefined ? {} : { content }) });
    };
    const wait = async (label: string, operation: string, path: string) => {
        record(label, operation, "start", path);
        if (label === "a" && input.phase === operation) {
            started.resolve();
            await gate.promise;
            if (input.reject !== undefined) {
                record(label, operation, "reject", path);
                throw new Error(input.reject);
            }
        }
    };
    const operations = (label: string) => ({
        mkdir: async (path: string) => {
            await wait(label, "mkdir", path);
            await mkdir(path, { recursive: true });
            record(label, "mkdir", "end", path);
        },
        writeFile: async (path: string, content: string) => {
            await wait(label, "write", path);
            await writeFile(path, content, "utf8");
            record(label, "write", "end", path, content);
        },
    });
    const first = api.createWriteToolDefinition(input.root, { operations: operations("a") });
    const second = api.createWriteToolDefinition(input.root, { operations: operations("b") });
    const path = input.path ?? "nested/file.txt";
    const other = input.different ? "nested/other.txt" : `nested/../${path}`;
    const outcome = async (promise: Promise<unknown>) => {
        try { return { ok: true, value: await promise }; }
        catch (error) { return { ok: false, error: (error instanceof Error ? error.message : String(error)).replaceAll(input.root, "ROOT") }; }
    };
    if (input.preabort) {
        controller.abort({ ignored: "signal reason" });
        const result = await outcome(first.execute("a", { path, content: "αA" }, controller.signal));
        return { events, results: [result], content: null };
    }
    let firstSettled = false;
    const a = outcome(first.execute("a", { path, content: "αA" }, controller.signal)).then(value => {
        firstSettled = true;
        return value;
    });
    await started.promise;
    const b = outcome(second.execute("b", { path: other, content: "B🐱" }));
    if (input.abort) controller.abort({ ignored: "signal reason" });
    if (input.different) await b;
    else await setImmediate();
    const before = { firstSettled, events: events.slice() };
    gate.resolve();
    const results = await Promise.all([a, b]);
    const content = await readFile(join(input.root, path), "utf8").catch(error => {
        if (error.code === "ENOENT") return null;
        throw error;
    });
    return { before, events, results, content };
}

void runOracle(async (module, request) => {
    const api = module as Api;
    const output = [];
    for (const input of requireCases(request)) output.push(await runScenario(api, input as Scenario));
    return output;
}).catch((error: unknown) => {
    process.stderr.write(`write-queue oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
