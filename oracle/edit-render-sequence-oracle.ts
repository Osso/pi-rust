import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[] };
type Step = { kind: "call" | "result" | "settle" | "quiet"; args?: unknown; complete?: boolean; result?: unknown; isError?: boolean };
type Input = { root: string; steps: Step[]; html?: boolean; awaitPreview?: boolean };
type Api = { createEditToolDefinition(cwd: string): {
    renderCall(args: unknown, theme: unknown, context: unknown): Component;
    renderResult(result: unknown, options: unknown, theme: unknown, context: unknown): Component;
} };
async function loadTheme() {
    const source = process.argv[2];
    if (!source) throw new Error("edit-render-sequence requires a source module");
    const theme = await import(pathToFileURL(join(dirname(source), "../../modes/interactive/theme/theme.ts")).href) as {
        initTheme(name: string): void; theme: unknown;
    };
    theme.initTheme("dark");
    return theme.theme;
}
async function renderHtmlHistory(api: Api, input: Input, theme: unknown) {
    const source = process.argv[2];
    const html = await import(pathToFileURL(join(dirname(source), "../export-html/tool-renderer.ts")).href);
    const definition = api.createEditToolDefinition(input.root);
    const renderer = html.createToolHtmlRenderer({getToolDefinition: () => definition, theme, cwd: input.root, width:80});
    const args = {path:"missing.txt",edits:[{oldText:"before",newText:"after"}]};
    const content = [{type:"text",text:"Could not edit file: missing.txt. Error code: ENOENT."}];
    const call = renderer.renderCall("edit-html", "edit", args);
    const immediate = renderer.renderResult("edit-html", "edit", content, undefined, true);
    if (!input.awaitPreview) return {call, immediate};
    const deadline = Date.now() + 5000;
    let settled = immediate;
    while (settled?.expanded !== "") {
        if (Date.now() >= deadline) throw new Error("HTML preview never reached result suppression");
        await new Promise(resolve => setTimeout(resolve, 5));
        settled = renderer.renderResult("edit-html", "edit", content, undefined, true);
    }
    return {call, immediate, settled};
}
const activeTheme = loadTheme();
void runOracle(async (module, request) => {
    const theme = await activeTheme;
    const output = [];
    for (const value of requireCases(request)) {
        const input = value as Input;
        if (input.html) {
            output.push(await renderHtmlHistory(module as Api, input, theme));
            continue;
        }
        const definition = (module as Api).createEditToolDefinition(input.root);
        const state = {};
        let call: Component | undefined;
        let result: Component | undefined;
        let args: unknown;
        let complete = true;
        let invalidations = 0;
        let consumed = 0;
        const frames = [];
        for (const step of input.steps) {
            if (step.kind === "quiet") await new Promise(resolve => setTimeout(resolve, 250));
            if (step.kind === "settle") {
                const deadline = Date.now() + 5000;
                while (invalidations <= consumed) {
                    if (Date.now() >= deadline) throw new Error("edit preview did not invalidate");
                    await new Promise(resolve => setTimeout(resolve, 5));
                }
                consumed = invalidations;
            }
            if (step.kind === "call") { args = step.args; complete = step.complete ?? true; }
            const context = {
                args, toolCallId: "edit-sequence", invalidate: () => { invalidations++; }, state,
                lastComponent: step.kind === "result" ? result : call,
                cwd: input.root, executionStarted: true, argsComplete: complete,
                isPartial: false, expanded: false, showImages: false, isError: step.isError ?? false,
            };
            let reused: boolean | null = null;
            if (step.kind === "result") {
                const next = definition.renderResult(step.result, {expanded:false,isPartial:false}, theme, context);
                reused = next === result;
                result = next;
            } else {
                call = definition.renderCall(args, theme, context);
            }
            frames.push({call:call?.render(80) ?? null,result:result?.render(80) ?? null,reused,invalidations});
        }
        output.push(frames);
    }
    return output;
}).catch((error: unknown) => {
    process.stderr.write(`edit-render-sequence oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
