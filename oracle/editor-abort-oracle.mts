import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
    const source = process.argv[2]!;
    const { Editor } = await import(pathToFileURL(`${source}/components/editor.ts`).href);
    const { TUI } = await import(pathToFileURL(`${source}/tui.ts`).href);
    const { VirtualTerminal } = await import(pathToFileURL(`${source}/../test/virtual-terminal.ts`).href);
    const flush = async () => { for (let i = 0; i < 12; i++) await Promise.resolve(); };
    const describe = (signal: AbortSignal) => signal.reason ? { name: signal.reason.name, message: signal.reason.message, code: signal.reason.code, dom: signal.reason instanceof DOMException } : null;
    async function run(): Promise<unknown> {
        const ui = new TUI(new VirtualTerminal(80, 24));
        const identity = (value: string) => value;
        const editor = new Editor(ui, { borderColor: identity, selectList: { selectedPrefix: identity, selectedText: identity, description: identity, scrollInfo: identity, noMatch: identity } });
        const events: unknown[] = [];
        const signals: AbortSignal[] = [];
        let release!: () => void;
        const held = new Promise<void>((resolve) => { release = resolve; });
        editor.setAutocompleteProvider({
            async getSuggestions(_lines: string[], _line: number, _col: number, { signal }: { signal: AbortSignal }) {
                const id = signals.length;
                signals.push(signal);
                events.push(["enter", id, signal.aborted, describe(signal)]);
                signal.addEventListener("abort", () => events.push(["abort", id, describe(signal)]));
                if (id === 0) await held;
                events.push(["return", id]);
                return null;
            },
            applyCompletion() { throw new Error("unexpected completion"); },
        });
        const snapshot = () => ({ events: structuredClone(events), signals: signals.map((signal) => ({ aborted: signal.aborted, reason: describe(signal) })) });
        editor.setText("first"); editor.handleInput("\t"); await flush();
        const output = [snapshot()];
        editor.setText("second"); editor.handleInput("\t"); await flush();
        output.push(snapshot());
        release(); await flush(); output.push(snapshot());
        editor.setText("done"); await flush(); output.push(snapshot());
        return output;
    }
    for await (const _line of createInterface({ input: process.stdin })) {
        try { process.stdout.write(`${JSON.stringify({ ok: true, output: await run() })}\n`); }
        catch (error) { process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`); }
    }
}
void main();
