import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
async function main(): Promise<void> {
    const source = process.argv[2]!;
    const { SessionSelectorComponent } = await import(pathToFileURL(`${source}/modes/interactive/components/session-selector.ts`).href);
    const { KeybindingsManager } = await import(pathToFileURL(`${source}/core/keybindings.ts`).href);
    const themes = await import(pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href);
    const tui = await import(pathToFileURL(`${source}/../../tui/src/index.ts`).href);
    async function run(request: { steps: Record<string, unknown>[] }): Promise<unknown> {
        themes.initTheme("dark", false);
        tui.setKeybindings(new KeybindingsManager());
        const events: unknown[] = [];
        const loads: { progress: (loaded: number, total: number) => void; resolve: (value: unknown[]) => void; reject: (error: Error) => void; scope: string }[] = [];
        const loader = (scope: string) => (progress: (loaded: number, total: number) => void) => {
            events.push(["load", scope]);
            return new Promise<unknown[]>((resolve, reject) => loads.push({ progress, resolve, reject, scope }));
        };
        const selector = new SessionSelectorComponent(loader("current"), loader("all"), (path: string) => events.push(["select", path]), () => events.push(["cancel"]), () => events.push(["exit"]), () => events.push(["render"]), {
            archivedSessionsLoader: loader("archived"),
            renameSession: async (path: string, name: string | undefined) => { events.push(["rename", path, name ?? null]); },
        });
        const snapshot = () => ({ lines: selector.render(90).map(units), children: selector.children.flatMap((child: { render(width: number): string[] }) => child.render(90)).map(units), events: structuredClone(events), selected: selector.getSessionList().getSelectedSessionPath() ?? null });
        const output = [snapshot()];
        for (const step of request.steps) {
            const index = Number(step.index ?? 0);
            if (step.op === "progress") loads[index]!.progress(Number(step.loaded), Number(step.total));
            else if (step.op === "resolve") loads[index]!.resolve([{ path: `/sessions/${loads[index]!.scope}.jsonl`, id: loads[index]!.scope, cwd: "/fixture", name: `name ${loads[index]!.scope}`, created: new Date(7258118400000), modified: new Date(7258118400000), messageCount: 2, firstMessage: "fixture" }]);
            else if (step.op === "reject") loads[index]!.reject(new Error(String(step.message)));
            else if (step.op === "key") selector.handleInput(String(step.data));
            else if (step.op === "error") selector.showError(String(step.message));
            else if (step.op === "focus") selector.focused = Boolean(step.value);
            else throw new Error(`unknown operation ${step.op}`);
            await flush();
            output.push(snapshot());
        }
        return output;
    }
    for await (const line of createInterface({ input: process.stdin })) {
        try { process.stdout.write(`${JSON.stringify({ ok: true, output: await run(JSON.parse(line)) })}\n`); }
        catch (error) { process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`); }
    }
}
void main();
