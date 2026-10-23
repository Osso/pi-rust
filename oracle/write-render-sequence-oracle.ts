import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[] };
type Step = { args?: unknown; result?: unknown; isError?: boolean; reset?: boolean; complete?: boolean; expanded?: boolean };
type RenderSequence = { steps: Step[]; width?: number; mode?: "result" };
type WriteApi = {
    createWriteToolDefinition(cwd: string): {
        renderCall(args: unknown, theme: unknown, context: unknown): Component;
        renderResult(result: unknown, options: unknown, theme: unknown, context: unknown): Component;
    };
};

async function loadDarkTheme(): Promise<unknown> {
    const source = process.argv[2];
    if (!source) throw new Error("write-render-sequence oracle requires a source module");
    const api = await import(pathToFileURL(join(dirname(source), "../../modes/interactive/theme/theme.ts")).href) as {
        initTheme(name: string): void; theme: unknown;
    };
    api.initTheme("dark");
    return api.theme;
}

const theme = loadDarkTheme();
void runOracle(async (module, request) => {
    const api = module as WriteApi;
    const activeTheme = await theme;
    return requireCases(request).map(value => {
        const sequence = value as RenderSequence;
        const definition = api.createWriteToolDefinition("/repo");
        let previous: Component | undefined;
        return sequence.steps.map(step => {
            if (step.reset) previous = undefined;
            const context = {
                args: step.args, toolCallId: "write-sequence", invalidate: () => {}, lastComponent: previous,
                state: {}, cwd: "/repo", executionStarted: false, argsComplete: step.complete ?? false,
                isPartial: true, expanded: step.expanded ?? true, showImages: false, isError: step.isError ?? false,
            };
            try {
                const component = sequence.mode === "result"
                    ? definition.renderResult(step.result, {expanded: context.expanded, isPartial: true}, activeTheme, context)
                    : definition.renderCall(step.args, activeTheme, context);
                const reused = component === previous;
                previous = component;
                const output = { ok: true, lines: component.render(sequence.width ?? 120) };
                return sequence.mode === "result" ? {...output, reused} : output;
            } catch (error) {
                if (!(error instanceof Error)) throw error;
                return { ok: false, error: error.message, constructor: error.constructor.name, text: String(error) };
            }
        });
    });
}).catch((error: unknown) => {
    process.stderr.write(`write-render-sequence oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
