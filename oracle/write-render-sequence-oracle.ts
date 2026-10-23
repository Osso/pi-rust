import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[] };
type Step = { args: unknown; complete?: boolean; expanded?: boolean };
type RenderSequence = { steps: Step[]; width?: number };
type WriteApi = {
    createWriteToolDefinition(cwd: string): {
        renderCall(args: unknown, theme: unknown, context: unknown): Component;
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
            const context = {
                args: step.args, toolCallId: "write-sequence", invalidate: () => {}, lastComponent: previous,
                state: {}, cwd: "/repo", executionStarted: false, argsComplete: step.complete ?? false,
                isPartial: true, expanded: step.expanded ?? true, showImages: false, isError: false,
            };
            try {
                const component = definition.renderCall(step.args, activeTheme, context);
                previous = component;
                return { ok: true, lines: component.render(sequence.width ?? 120) };
            } catch (error) {
                return { ok: false, error: error instanceof Error ? error.message : String(error) };
            }
        });
    });
}).catch((error: unknown) => {
    process.stderr.write(`write-render-sequence oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
