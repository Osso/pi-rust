import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Step = { op: string; units?: number[] };
type Request = { config: Record<string, any>; images: boolean; colors: Record<string, string>; previewColors: Record<string, string>; preview: boolean; changeThemeOnPreview: boolean; fail: string; width: number; children?: boolean; steps: Step[] };
const text = (value: number[]): string => String.fromCharCode(...value);
const units = (value: string): number[] => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
const number = (value: number | string): number => typeof value === "number" ? value : Number(value);

async function main(): Promise<void> {
    const source = process.argv[2]!;
    const tui = await import(pathToFileURL(`${source}/../../tui/src/index.ts`).href);
    const { SettingsSelectorComponent } = await import(pathToFileURL(`${source}/modes/interactive/components/settings-selector.ts`).href);
    const { KeybindingsManager } = await import(pathToFileURL(`${source}/core/keybindings.ts`).href);
    const themes = await import(pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href);
    function run(request: Request): unknown {
        tui.setKeybindings(new KeybindingsManager());
        tui.setCapabilities({ images: request.images ? "kitty" : null, trueColor: true, hyperlinks: false });
        themes.setThemeInstance(new themes.Theme(request.colors, {}, "truecolor"));
        const events: unknown[] = [];
        const callbacks = new Proxy({}, { get: (_target, name) => {
            if (name === "onThemePreview" && !request.preview) return undefined;
            return (...args: unknown[]): void => {
                events.push([name, args.length === 0 ? null : JSON.stringify(args[0])]);
                if (name === "onThemePreview" && request.changeThemeOnPreview) themes.setThemeInstance(new themes.Theme(request.previewColors, {}, "truecolor"));
                if (name === request.fail) throw new Error(`callback ${String(name)}`);
            };
        } });
        const config = { ...request.config,
            currentTheme: text(request.config.currentTheme), availableThemes: request.config.availableThemes.map(text),
            imageWidthCells: number(request.config.imageWidthCells), editorPaddingX: number(request.config.editorPaddingX),
            autocompleteMaxVisible: number(request.config.autocompleteMaxVisible), httpIdleTimeoutMs: number(request.config.httpIdleTimeoutMs),
            warnings: JSON.parse(request.config.warningsJson),
        };
        const component = new SettingsSelectorComponent(config, callbacks);
        // Restore only the row and callback from 532f7d12 settings-selector.ts:543-549, 821-823.
        // Keep the common-pin SettingsList and every other selector behavior unchanged.
        const list = component.getSettingsList() as any;
        const position = list.items.findIndex((item: { id: string }) => item.id === "hide-thinking");
        if (position < 0 || list.items !== list.filteredItems) throw new Error("unexpected common-pin settings list");
        list.items.splice(position + 1, 0, {
            id: "mermaid-rendering", label: "Mermaid diagrams",
            description: "Render Mermaid code blocks as Unicode diagrams",
            currentValue: config.mermaidRenderingMode, values: ["off", "final", "streaming"],
        });
        const onChange = list.onChange;
        list.onChange = (id: string, value: string) => {
            if (id === "mermaid-rendering") (callbacks as any).onMermaidRenderingModeChange(value);
            else onChange(id, value);
        };
        let invalidations = 0;
        const extra = { render: () => [`extra${invalidations}${String.fromCharCode(0xd800)}`], invalidate: () => { invalidations++; } };
        const snapshot = (error?: string): unknown => ({
            lines: component.render(request.width).map(units), events: [...events],
            list: component.getSettingsList().render(request.width).map(units),
            ...(request.children ? { children: component.children.flatMap((child: { render(width: number): string[] }) => child.render(request.width)).map(units) } : {}),
            ...(error ? { error } : {}),
        });
        const output = [snapshot()];
        for (const step of request.steps) {
            let error: string | undefined;
            try {
                switch (step.op) {
                    case "key": component.getSettingsList().handleInput(text(step.units!)); break;
                    case "invalidate": component.invalidate(); break;
                    case "add": component.addChild(extra); break;
                    case "remove": component.removeChild(extra); break;
                    case "clear": component.clear(); break;
                    default: throw new Error(`unknown step ${step.op}`);
                }
            } catch (caught) { error = String(caught); }
            output.push(snapshot(error));
        }
        return output;
    }
    for await (const line of createInterface({ input: process.stdin })) {
        try { process.stdout.write(`${JSON.stringify({ ok: true, output: run(JSON.parse(line)) })}\n`); }
        catch (error) { process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`); }
    }
}
void main();
