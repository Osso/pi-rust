import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Block = { kind: "text" | "thinking" | "toolCall"; units?: number[]; signature?: string | null };
type Message = { content: Block[]; stop: string; errorUnits?: number[] };
type Step = { op: "update" | "hide" | "label" | "pad" | "invalidate" | "render"; message?: Message; value?: number | boolean; units?: number[]; width?: number };
type Request = { initial: Message | null; hide: boolean; label: number[]; padding: number; width: number; steps: Step[]; stateful?: boolean };
const text = (units: number[]): string => String.fromCharCode(...units);
const units = (value: string): number[] => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));

async function main(): Promise<void> {
    const source = process.argv[2]!;
    const { AssistantMessageComponent } = await import(pathToFileURL(`${source}/modes/interactive/components/assistant-message.ts`).href);
    const themes = await import(pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href);
    themes.setThemeInstance(new themes.Theme({ thinkingText: 0, error: 0 }, {}, "256color"));
    function message(value: Message): unknown {
        return {
            role: "assistant", api: "fixture", provider: "fixture", model: "fixture", timestamp: 0,
            content: value.content.map(block => block.kind === "text" ? { type: "text", text: text(block.units!) }
                : block.kind === "thinking" ? { type: "thinking", thinking: text(block.units!), thinkingSignature: block.signature ?? undefined }
                : { type: "toolCall", id: "tool-1", name: "read", arguments: {} }),
            stopReason: value.stop, errorMessage: value.errorUnits === undefined ? undefined : text(value.errorUnits),
        };
    }
    function observe(request: Request): unknown {
        let highlighted = 0;
        const identity = (value: string): string => value;
        const theme = {
            heading: identity, link: identity, linkUrl: identity, code: identity, codeBlock: identity,
            codeBlockBorder: identity, quote: identity, quoteBorder: identity, hr: identity, listBullet: identity,
            bold: identity, italic: identity, strikethrough: identity, underline: identity,
            highlightCode: request.stateful ? (code: string): string[] => [`[${++highlighted}]${code}`] : undefined,
        };
        const component = new AssistantMessageComponent(request.initial === null ? undefined : message(request.initial), request.hide, theme, text(request.label), request.padding);
        let width = request.width;
        const snapshot = (): number[][] => component.render(width).map(units);
        const snapshots = [snapshot()];
        for (const step of request.steps) {
            switch (step.op) {
                case "update": component.updateContent(message(step.message!)); break;
                case "hide": component.setHideThinkingBlock(step.value); break;
                case "label": component.setHiddenThinkingLabel(text(step.units!)); break;
                case "pad": component.setOutputPad(step.value); break;
                case "invalidate": component.invalidate(); break;
                case "render": break;
            }
            width = step.width ?? width;
            snapshots.push(snapshot());
        }
        return snapshots;
    }
    for await (const line of createInterface({ input: process.stdin })) {
        try { process.stdout.write(`${JSON.stringify({ ok: true, output: observe(JSON.parse(line)) })}\n`); }
        catch (error) { process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`); }
    }
}
void main();
