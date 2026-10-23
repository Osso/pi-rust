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
    async function run(request: { steps: Record<string, unknown>[]; width?: number; rejectRename?: boolean; noArchived?: boolean }): Promise<unknown> {
        const width = request.width ?? 90;
        const fixture = (scope: string, item: Record<string, unknown> = {}) => ({ path: item.path ?? `/sessions/${scope}.jsonl`, id: scope, cwd: item.cwd ?? "/fixture", name: item.nameUnits ? String.fromCharCode(...item.nameUnits as number[]) : item.name === null ? undefined : item.name ?? `name ${scope}`, parentSessionPath: item.parent, created: new Date(7258118400000), modified: new Date(7258118400000), messageCount: item.count ?? 2, firstMessage: "fixture" });
        themes.initTheme("dark", false);
        tui.setKeybindings(new KeybindingsManager());
        const events: unknown[] = [];
        const loads: { progress: (loaded: number, total: number) => void; resolve: (value: unknown[]) => void; reject: (error: Error) => void; scope: string }[] = [];
        const loader = (scope: string) => (progress: (loaded: number, total: number) => void) => {
            events.push(["load", scope]);
            return new Promise<unknown[]>((resolve, reject) => loads.push({ progress, resolve, reject, scope }));
        };
        const selector = new SessionSelectorComponent(loader("current"), loader("all"), (path: string) => events.push(["select", path]), () => events.push(["cancel"]), () => events.push(["exit"]), () => events.push(["render"]), {
            archivedSessionsLoader: request.noArchived ? undefined : loader("archived"),
            renameSession: async (path: string, name: string | undefined) => { events.push(["rename", path, name ?? null]); if (request.rejectRename) throw new Error("rename rejected"); },
        });
        const extra = new tui.Text(`extra${String.fromCharCode(0xd800)}`, 0, 0);
        const snapshot = () => ({ lines: selector.render(width).map(units), children: selector.children.flatMap((child: { render(width: number): string[] }) => child.render(width)).map(units), events: structuredClone(events), selected: selector.getSessionList().getSelectedSessionPath() ?? null });
        const output = [snapshot()];
        for (const step of request.steps) {
            const index = Number(step.index ?? 0);
            if (step.op === "progress") loads[index]!.progress(Number(step.loaded), Number(step.total));
            else if (step.op === "resolve") loads[index]!.resolve(((step.items ?? [{}]) as Record<string, unknown>[]).map((item) => fixture(loads[index]!.scope, item)));
            else if (step.op === "reject") loads[index]!.reject(new Error(String(step.message)));
            else if (step.op === "key") selector.handleInput(String(step.data));
            else if (step.op === "listKey") selector.getSessionList().handleInput(String(step.data));
            else if (step.op === "listFocus") selector.getSessionList().focused = Boolean(step.value);
            else if (step.op === "error") selector.showError(String(step.message));
            else if (step.op === "focus") selector.focused = Boolean(step.value);
            else if (step.op === "wait") await new Promise((resolve) => setTimeout(resolve, Number(step.ms)));
            else if (step.op === "add") selector.addChild(extra);
            else if (step.op === "remove") selector.removeChild(extra);
            else if (step.op === "clear") selector.clear();
            else if (step.op === "invalidate") selector.invalidate();
            else if (step.op === "confirmRejected") {
                try { await selector.confirmRename(String(step.value)); }
                catch (error) { events.push(["rejected", error instanceof Error ? error.message : String(error)]); }
            }
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
