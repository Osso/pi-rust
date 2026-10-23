import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { existsSync } from "node:fs";

async function main(): Promise<void> {
    const source = process.argv[2]!;
    const { SessionSelectorComponent } = await import(pathToFileURL(`${source}/modes/interactive/components/session-selector.ts`).href);
    const { KeybindingsManager } = await import(pathToFileURL(`${source}/core/keybindings.ts`).href);
    const themes = await import(pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href);
    const { readSessionMetadata } = await import(pathToFileURL(`${source}/core/session-control-db.ts`).href);
    const tui = await import(pathToFileURL(`${source}/../../tui/src/index.ts`).href);
    async function run(request: { paths: string[]; db: string; operation: string; withDb: boolean; archived: boolean }): Promise<unknown> {
        themes.initTheme("dark", false);
        tui.setKeybindings(new KeybindingsManager());
        const events: unknown[] = [];
        const loader = async () => {
            events.push(["load"]);
            return request.paths.slice(0, 2).map((path, index) => ({ path, id: String(index), cwd: "/fixture", name: index ? "child" : "root", isArchived: request.archived, created: new Date(7258118400000), modified: new Date(7258118400000), messageCount: 2, firstMessage: "fixture" })).filter((session) => existsSync(session.path));
        };
        const selector = new SessionSelectorComponent(loader, loader, () => {}, () => {}, () => {}, () => events.push(["render"]), { controlDbPath: request.withDb ? request.db : undefined });
        for (let i = 0; i < 12; i++) await Promise.resolve();
        if (request.operation === "delete") await selector.getSessionList().onDeleteSession(request.paths[0]);
        else await selector.getSessionList().onArchiveSession(request.paths[0]);
        return {
            frames: selector.render(90).map((line: string) => Array.from({ length: line.length }, (_, i) => line.charCodeAt(i))),
            events,
            files: request.paths.map(existsSync),
            metadata: request.paths.map((path) => { const row = readSessionMetadata(request.db, path); return row ? [row.sessionPath, row.isArchived ?? false, row.parentSessionPath ?? null] : null; }),
        };
    }
    for await (const line of createInterface({ input: process.stdin })) {
        try { process.stdout.write(`${JSON.stringify({ ok: true, output: await run(JSON.parse(line)) })}\n`); }
        catch (error) { process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`); }
    }
}
void main();
