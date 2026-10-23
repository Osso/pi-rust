import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { root: string; phase: "access" | "read" | "write"; message: string;
    class?: "Error" | "TypeError"; name?: string; code?: unknown; abort?: boolean };
type Api = { createEditToolDefinition(cwd: string, options: unknown): {
    execute(id: string, params: unknown, signal: AbortSignal): Promise<unknown>;
} };

void runOracle(async (module, request) => {
    const api = module as Api;
    const output = [];
    for (const value of requireCases(request)) {
        const input = value as Input;
        const calls: string[] = [];
        const controller = new AbortController();
        const fail = (phase: string) => {
            calls.push(phase);
            if (input.phase !== phase) return;
            const error = input.class === "TypeError" ? new TypeError(input.message) : new Error(input.message);
            if (input.name !== undefined) error.name = input.name;
            if (Object.hasOwn(input, "code")) Object.assign(error, {code: input.code});
            if (input.abort) controller.abort("ignored reason");
            throw error;
        };
        const tool = api.createEditToolDefinition(input.root, {operations: {
            access: async () => { fail("access"); },
            readFile: async () => { fail("read"); return Buffer.from("before"); },
            writeFile: async () => { fail("write"); },
        }});
        try {
            await tool.execute("edit-errors", {path:"raw-\udfff.txt", edits:[{oldText:"before",newText:"after"}]}, controller.signal);
            throw new Error("error fixture unexpectedly succeeded");
        } catch (error) {
            if (!(error instanceof Error)) throw error;
            output.push({calls, error:error.message});
        }
    }
    return output;
}).catch((error: unknown) => {
    process.stderr.write(`edit-operation-errors oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
