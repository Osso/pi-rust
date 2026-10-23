import { access, readFile, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { join } from "node:path";
import { setImmediate } from "node:timers/promises";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { root: string; phase: "access" | "read" | "write"; abort: boolean; reject?: boolean; different?: boolean };
type Api = { createEditToolDefinition(cwd: string, options: unknown): {
    execute(id: string, args: unknown, signal?: AbortSignal): Promise<unknown>;
} };

async function runScenario(api: Api, input: Input) {
    await writeFile(join(input.root, "file.txt"), "before\nkeep\n");
    await writeFile(join(input.root, "other.txt"), "before\nkeep\n");
    const events: string[] = [];
    const gate = Promise.withResolvers<void>();
    const started = Promise.withResolvers<void>();
    const controller = new AbortController();
    const enter = async (label: string, phase: string) => {
        events.push(`${label}:${phase}:start`);
        if (label === "a" && phase === input.phase) {
            started.resolve();
            await gate.promise;
            if (input.reject) {
                events.push(`${label}:${phase}:reject`);
                throw new Error("operation failed");
            }
        }
    };
    const operations = (label: string) => ({
        access: async (path: string) => {
            await enter(label, "access");
            await access(path, constants.R_OK | constants.W_OK);
            events.push(`${label}:access:end`);
        },
        readFile: async (path: string) => {
            await enter(label, "read");
            const bytes = await readFile(path);
            events.push(`${label}:read:end`);
            return bytes;
        },
        writeFile: async (path: string, content: string) => {
            await enter(label, "write");
            await writeFile(path, content, "utf8");
            events.push(`${label}:write:end`);
        },
    });
    const outcome = async (promise: Promise<unknown>) => {
        try { return { ok: true, value: await promise }; }
        catch (error) {
            if (!(error instanceof Error)) throw error;
            return { ok: false, error: error.message.replaceAll(input.root, "ROOT") };
        }
    };
    const first = api.createEditToolDefinition(input.root, { operations: operations("a") });
    const second = api.createEditToolDefinition(input.root, { operations: operations("b") });
    let settled = false;
    const a = outcome(first.execute("a", {
        path: "file.txt", edits: [{ oldText: "before", newText: "first" }],
    }, controller.signal)).then(value => { settled = true; return value; });
    await started.promise;
    const b = outcome(second.execute("b", {
        path: input.different ? "other.txt" : "./file.txt", edits: [{ oldText: "keep", newText: "second" }],
    }));
    if (input.abort) controller.abort("ignored reason");
    const independent = input.different ? await b : undefined;
    await setImmediate();
    const before = { settled, events: [...events] };
    gate.resolve();
    const results = [await a, independent ?? await b];
    const contents = await Promise.all(["file.txt", "other.txt"].map(path => readFile(join(input.root, path), "utf8")));
    return { before, events, results, contents };
}

void runOracle(async (module, request) => {
    const output = [];
    for (const value of requireCases(request)) output.push(await runScenario(module as Api, value as Input));
    return output;
}).catch((error: unknown) => {
    process.stderr.write(`edit-queue oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
