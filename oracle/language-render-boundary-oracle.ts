import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Widget = {
    render(width: number): string[];
    setExpanded(expanded: boolean): void;
    updateResult(result: unknown, partial: boolean): void;
};
type Factory = (cwd: string) => unknown;
type HtmlRenderer = {
    renderCall(id: string, name: string, args: unknown): unknown;
    renderResult(id: string, name: string, content: unknown[], details: undefined, error: boolean): unknown;
};

async function loadDependencies() {
    const source = process.argv[2];
    if (!source) throw new Error("language-render-boundary oracle requires a source module");
    const load = (relative: string) => import(pathToFileURL(join(dirname(source), relative)).href);
    const write = await load("write.ts") as { createWriteToolDefinition: Factory };
    const read = await load("read.ts") as { createReadToolDefinition: Factory };
    const theme = await load("../../modes/interactive/theme/theme.ts") as { initTheme(name: string): void; theme: unknown };
    const execution = await load("../../modes/interactive/components/tool-execution.ts") as {
        ToolExecutionComponent: new (name: string, id: string, args: unknown, options: unknown, definition: unknown, ui: unknown, cwd: string) => Widget;
    };
    const html = await load("../export-html/tool-renderer.ts") as { createToolHtmlRenderer(deps: unknown): HtmlRenderer };
    theme.initTheme("dark");
    return { write, read, theme, execution, html };
}

const dependencies = loadDependencies();
void runOracle(async (_module, request) => {
    const { write, read, theme, execution, html } = await dependencies;
    return requireCases(request).map(value => {
        const input = value as { tool: "read" | "write"; path: string };
        const args = { path: input.path, content: "plain" };
        const definition = input.tool === "write" ? write.createWriteToolDefinition("/repo") : read.createReadToolDefinition("/repo");
        const widget = new execution.ToolExecutionComponent(input.tool, "language-error", args, {}, definition, { requestRender() {} }, "/repo");
        const exporter = html.createToolHtmlRenderer({ getToolDefinition: () => definition, theme: theme.theme, cwd: "/repo", width: 80 });
        const call = exporter.renderCall("language-error", input.tool, args);
        let htmlValue = call;
        if (input.tool === "read") {
            widget.setExpanded(true);
            const content = [{ type: "text", text: "literal body" }];
            widget.updateResult({ content, isError: true }, false);
            htmlValue = exporter.renderResult("language-error", "read", content, undefined, true);
        }
        return { lines: widget.render(80), html: htmlValue ?? null };
    });
}).catch((error: unknown) => {
    process.stderr.write(`language-render-boundary oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
