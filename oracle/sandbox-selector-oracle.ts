import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Request = { current: string; session?: string; width: number; keys: string[]; platform: string };
const units = (text: string): number[] => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
async function main(): Promise<void> {
    const source = process.argv[2]!;
    const { SandboxSelectorComponent } = await import(pathToFileURL(`${source}/modes/interactive/components/sandbox-selector.ts`).href);
    const themes = await import(pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href);
    themes.setThemeInstance(new themes.Theme({ accent: 0, muted: 0, text: 0, success: 0, border: 0, dim: 0 }, {}, "256color"));
    for await (const line of createInterface({ input: process.stdin })) {
        try {
            const request = JSON.parse(line) as Request;
            Object.defineProperty(process, "platform", { value: request.platform, configurable: true });
            const events: string[] = [];
            const component = new SandboxSelectorComponent({
                currentProfile: request.current, sessionProfile: request.session ?? undefined,
                onSelect: (value: { profile?: string; scope: string }) => events.push(`select:${value.profile ?? "none"}:${value.scope}`),
                onCancel: () => events.push("cancel"),
            });
            const snapshot = () => ({
                lines: component.render(request.width).map(units),
                children: component.children.flatMap((child: { render(width: number): string[] }) => child.render(request.width)).map(units),
                events: [...events],
            });
            const output = [snapshot()];
            for (const key of request.keys) { component.handleInput(key); component.invalidate(); output.push(snapshot()); }
            let invalidations = 0;
            const extra = { render: () => [`extra${invalidations}${String.fromCharCode(0xd800)}`], invalidate: () => { invalidations++; } };
            component.addChild(extra); output.push(snapshot());
            component.invalidate(); output.push(snapshot());
            component.removeChild(extra); output.push(snapshot());
            component.clear(); output.push(snapshot());
            component.handleInput("\t"); output.push(snapshot());
            process.stdout.write(`${JSON.stringify({ ok: true, output })}\n`);
        } catch (error) { process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`); }
    }
}
void main();
