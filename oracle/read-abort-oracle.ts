import { setImmediate as nextTurn } from "node:timers/promises";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { stage: string; fail?: boolean; range?: boolean; image?: boolean };
type Api = { createReadToolDefinition(cwd: string, options: {
    autoResizeImages: boolean;
    operations: {
        access(): Promise<void>;
        detectImageMimeType(): Promise<string | undefined>;
        stat(): Promise<{size: number}>;
        readFile(): Promise<Buffer>;
        readRange(): Promise<Buffer>;
    };
}): {execute(id: string, args: {path: string}, signal: AbortSignal): Promise<unknown>} };

async function runCase(api: Api, cwd: string, input: Case) {
    const trace: string[] = [];
    let entered!: () => void;
    const pending = new Promise<void>((resolve) => { entered = resolve; });
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    let completed!: () => void;
    const final = new Promise<void>((resolve) => { completed = resolve; });
    const finalStage = input.fail || input.stage === "access" ? input.stage : input.range ? "range" : "read";
    async function step(stage: string) {
        trace.push(`${stage}:start`);
        if (stage === input.stage) { entered(); await gate; }
        trace.push(`${stage}:end`);
        if (stage === finalStage) completed();
        if (input.fail && stage === input.stage) throw new Error(`late ${stage} failure`);
    }
    const definition = api.createReadToolDefinition(cwd, { autoResizeImages: false, operations: {
        access: async () => { await step("access"); },
        detectImageMimeType: async () => { await step("mime"); return input.image ? "image/png" : undefined; },
        stat: async () => { await step("stat"); return { size: input.range ? 20000 : 4 }; },
        readFile: async () => { await step("read"); return Buffer.from("head"); },
        readRange: async () => { await step("range"); return Buffer.from("head"); },
    } });
    const controller = new AbortController();
    const task = definition.execute("abort-read", { path: "fixture" }, controller.signal)
        .then((value) => ({ value }), (error: Error) => ({ error: error.message }));
    await pending;
    controller.abort("custom reason ignored by read");
    const early = await Promise.race([task, new Promise<string>((resolve) => setTimeout(() => resolve("pending"), 40))]);
    const beforeRelease = [...trace];
    release();
    await final;
    await nextTurn();
    return { early, beforeRelease, settled: await task, trace };
}
void runOracle(async (module, request) => {
    const output = [];
    for (const input of requireCases(request)) output.push(await runCase(module as unknown as Api, String(request.cwd), input as Case));
    return output;
});
