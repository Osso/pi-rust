import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const units = (value: string): number[] => Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
const text = (value: number[]) => String.fromCharCode(...value);
async function main(): Promise<void> {
    const source = process.argv[2]!;
    const tui = await import(pathToFileURL(`${source}/../../tui/src/index.ts`).href);
    const { BorderedLoader } = await import(pathToFileURL(`${source}/modes/interactive/components/bordered-loader.ts`).href);
    const { KeybindingsManager } = await import(pathToFileURL(`${source}/core/keybindings.ts`).href);
    const themes = await import(pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href);
    function run(request: any): unknown {
        tui.setKeybindings(new KeybindingsManager());
        const captured = new themes.Theme(request.colors, request.colors, "truecolor");
        const other = new themes.Theme(request.otherColors, request.otherColors, "truecolor");
        themes.setThemeInstance(request.separateTheme ? other : captured);
        let renders = 0;
        const ui = { requestComponentRender() { renders++; }, requestRender() { renders++; } };
        const component = request.bare ? new tui.CancellableLoader(ui, (value: string) => captured.fg("accent", value), (value: string) => captured.fg("muted", value), text(request.message), { frames: [] }) : new BorderedLoader(ui, captured, text(request.message), { cancellable: request.cancellable });
        const signal = component.signal;
        const reason = () => signal.reason ? { dom: signal.reason instanceof DOMException, name: signal.reason.name, message: signal.reason.message, code: signal.reason.code } : null;
        const events: unknown[] = [];
        signal.addEventListener("abort", () => events.push(["abort", reason()]));
        const handler = (tag: string) => () => events.push(["callback", tag, signal.aborted, reason()]);
        component.onAbort = handler("first");
        const extra = new tui.Text(`extra${String.fromCharCode(0xd800)}`, 0, 0);
        const snapshot = () => ({ lines: component.render(50).map(units), children: component.children ? component.children.flatMap((child: any) => child.render(50)).map(units) : null, sameSignal: signal === component.signal, aborted: signal.aborted, reason: reason(), events: [...events], renders });
        const output = [snapshot()];
        try {
            for (const step of request.steps) {
                switch (step.op) {
                    case "key": component.handleInput(text(step.units)); break;
                    case "handler": component.onAbort = step.tag === null ? undefined : handler(step.tag); break;
                    case "theme": themes.setThemeInstance(other); break;
                    case "invalidate": component.invalidate(); break;
                    case "dispose": component.dispose(); break;
                    case "add": component.addChild(extra); break;
                    case "remove": component.removeChild(extra); break;
                    case "clear": component.clear(); break;
                    default: throw new Error(`unknown operation ${step.op}`);
                }
                output.push(snapshot());
            }
            return output;
        } finally { component.dispose(); }
    }
    for await (const line of createInterface({ input: process.stdin })) {
        try { process.stdout.write(`${JSON.stringify({ ok: true, output: run(JSON.parse(line)) })}\n`); }
        catch (error) { process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`); }
    }
}
void main();
