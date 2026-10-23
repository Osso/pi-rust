import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
const text = (value: number[]) => String.fromCharCode(...value);
async function main(): Promise<void> {
    const source = process.argv[2]!;
    const { TreeSelectorComponent } = await import(pathToFileURL(`${source}/modes/interactive/components/tree-selector.ts`).href);
    const { KeybindingsManager } = await import(pathToFileURL(`${source}/core/keybindings.ts`).href);
    const themes = await import(pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href);
    const tui = await import(pathToFileURL(`${source}/../../tui/src/index.ts`).href);
    async function run(request: any): Promise<unknown> {
        themes.initTheme("dark", false);
        tui.setKeybindings(new KeybindingsManager(request.bindings ?? {}));
        const nodes = request.nodes.map((item: any) => {
            const entry = item.entry;
            if (item.contentUnits) entry.message.content = text(item.contentUnits);
            if (item.textUnits) entry.message.content[0].text = text(item.textUnits);
            if (item.argumentUnits) for (const [key, value] of Object.entries(item.argumentUnits)) entry.message.content[0].arguments[key] = text(value as number[]);
            return { entry, label: item.labelUnits ? text(item.labelUnits) : item.label, labelTimestamp: item.labelTimestamp, children: [] };
        });
        const byId = new Map(nodes.map((node: any) => [node.entry.id, node]));
        const roots: unknown[] = [];
        for (const node of nodes) {
            const parent = byId.get(node.entry.parentId) as any;
            if (parent) parent.children.push(node); else roots.push(node);
        }
        const events: unknown[] = [];
        let selector: any = new TreeSelectorComponent(roots, request.leaf ?? null, Number(request.height ?? 24), (id: string) => events.push(["select", units(id)]), () => events.push(["cancel"]), (id: string, label: string | undefined) => events.push(["label", units(id), label === undefined ? null : units(label)]), request.selected ?? undefined, request.filter ?? undefined);
        const list = selector.getTreeList();
        const extra = new tui.Text(`extra${String.fromCharCode(0xd800)}`, 0, 0);
        const width = request.width ?? 90;
        const snapshot = () => ({
            lines: selector.render(width).map(units),
            rows: list.render(width).map(units),
            children: selector.children.flatMap((child: any) => child.render(width)).map(units),
            selected: list.getSelectedNode()?.entry.id ?? null,
            label: list.getSelectedNode()?.label === undefined ? null : units(list.getSelectedNode().label),
            search: units(list.getSearchQuery()),
            events: structuredClone(events),
        });
        const output = [snapshot()];
        if (request.drop) {
            selector = undefined;
            await new Promise((resolve) => setTimeout(resolve, 130));
            output.push({ events: structuredClone(events) });
            return output;
        }
        for (const step of request.steps) {
            if (step.op === "key") selector.handleInput(text(step.units));
            else if (step.op === "listKey") list.handleInput(text(step.units));
            else if (step.op === "focus") selector.focused = step.value;
            else if (step.op === "label") list.updateNodeLabel(step.id, step.labelUnits === null ? undefined : text(step.labelUnits), step.timestamp);
            else if (step.op === "wait") await new Promise((resolve) => setTimeout(resolve, step.ms));
            else if (step.op === "add") selector.addChild(extra);
            else if (step.op === "remove") selector.removeChild(extra);
            else if (step.op === "clear") selector.clear();
            else if (step.op === "invalidate") selector.invalidate();
            else throw new Error(`unknown operation ${step.op}`);
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
