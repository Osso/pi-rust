import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const units = (value: string): number[] => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
async function main(): Promise<void> {
    const source = process.argv[2]!;
    const tui = await import(pathToFileURL(`${source}/../../tui/src/index.ts`).href);
    const { ToolExecutionComponent } = await import(pathToFileURL(`${source}/modes/interactive/components/tool-execution.ts`).href);
    const themes = await import(pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href);
    function run(request: any): unknown {
        tui.setCapabilities({ images: null, trueColor: true, hyperlinks: false });
        themes.setThemeInstance(new themes.Theme(request.colors, request.colors, "truecolor"));
        const events: unknown[] = [];
        let renders = 0;
        const record = (name: string, context: any): void => {
            context.state[name] = (context.state[name] ?? 0) + 1;
            events.push([name, JSON.stringify(context.args), context.toolCallId, context.cwd, context.executionStarted, context.argsComplete, context.isPartial, context.expanded, context.showImages, context.isError, !!context.lastComponent, JSON.stringify(context.state)]);
        };
        const definition = request.definition === "none" ? undefined : {
            name: request.name, renderShell: request.definition,
            ...(request.call ? { renderCall: (_args: unknown, _theme: unknown, context: any) => {
                record("call", context);
                if (request.throwCall) throw new Error("call");
                const component = context.lastComponent ?? new tui.Text("", 0, 0);
                component.setText(request.emptyCall ? "" : `call ${context.state.call}`);
                return component;
            } } : {}),
            ...(request.result ? { renderResult: (result: unknown, _options: unknown, _theme: unknown, context: any) => {
                record("result", context);
                if (request.throwResult) throw new Error("result");
                const component = context.lastComponent ?? new tui.Text("", 0, 0);
                component.setText(JSON.stringify(result));
                return component;
            } } : {}),
        };
        const component = new ToolExecutionComponent(request.name, "call-id", JSON.parse(request.argsJson), request.options, definition, { requestRender() { renders++; } }, "/repo");
        const snapshot = () => ({ lines: component.render(request.width).map(units), events: [...events], renders });
        const output = [snapshot()];
        for (const step of request.steps) {
            switch (step.op) {
                case "args": component.updateArgs(JSON.parse(step.json)); break;
                case "complete": component.setArgsComplete(); break;
                case "start": component.markExecutionStarted(step.at, { showLiveElapsed: step.live }); break;
                case "result": component.updateResult(JSON.parse(step.json), step.partial, step.at); break;
                case "expanded": component.setExpanded(step.value); break;
                case "images": component.setShowImages(step.value); break;
                case "hide": component.setHideOutput(step.value); break;
                case "width": component.setImageWidthCells(step.value); break;
                case "invalidate": component.invalidate(); break;
                default: throw new Error(`unknown operation ${step.op}`);
            }
            output.push(snapshot());
        }
        return output;
    }
    for await (const line of createInterface({ input: process.stdin })) {
        try { process.stdout.write(`${JSON.stringify({ ok: true, output: run(JSON.parse(line)) })}\n`); }
        catch (error) { process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`); }
    }
}
void main();
