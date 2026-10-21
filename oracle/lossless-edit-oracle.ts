import { createInterface } from "node:readline";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

async function main() {
    const edit = await import(pathToFileURL(process.argv[2]).href);
    const diff = createRequire(process.argv[2])("diff");
    const text = (units: number[]) => String.fromCharCode(...units);
    for await (const line of createInterface({ input: process.stdin })) {
        const request: { kind?: "words"; content: number[]; oldText: number[]; newText: number[]; path: number[] } = JSON.parse(line);
        if (request.kind === "words") {
            const parts = diff.diffWords(text(request.oldText), text(request.newText)).map((part: { added?: boolean; removed?: boolean; value: string }) => ({ added: !!part.added, removed: !!part.removed, value: part.value }));
            console.log(JSON.stringify({ wire: JSON.stringify(parts) }));
            continue;
        }
        const content = text(request.content);
        const path = text(request.path);
        try {
            const applied = edit.applyEditsToNormalizedContent(content, [{ oldText: text(request.oldText), newText: text(request.newText) }], path);
            const diff = edit.generateDiffString(applied.baseContent, applied.newContent, 4);
            const patch = edit.generateUnifiedPatch(path, applied.baseContent, applied.newContent, 4);
            console.log(JSON.stringify({ wire: JSON.stringify({ baseContent: applied.baseContent, newContent: applied.newContent, diff, patch }) }));
        } catch (error) {
            console.log(JSON.stringify({ wire: JSON.stringify({ error: error instanceof Error ? error.message : String(error) }) }));
        }
    }
}
main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});
