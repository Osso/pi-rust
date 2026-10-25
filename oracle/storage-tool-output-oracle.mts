import { createInterface } from "node:readline";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { utimesSync, writeFileSync } from "node:fs";

const root = process.argv[2];
const tools = await import(pathToFileURL(join(root, "session-tool-output.ts")).href);
const manager = await import(pathToFileURL(join(root, "session-manager.ts")).href);

type Request = { id: number; op: string; dir: string; text?: string; path?: string; advanceMtime?: number; entry?: unknown };
function runRequest(request: Request): unknown {
    if (request.op === "truncate" || request.op === "serialize") {
        try {
            if (request.op === "serialize") return { ok: true, line: tools.serializeSessionEntryForPersistence(request.entry) };
            return { ok: true, entry: tools.truncateToolResultForPersistence(request.entry) };
        } catch (error) {
            if (!(error instanceof Error)) throw error;
            return { ok: false, error: { name: error.name, message: error.message,
                ...Object.fromEntries(Object.entries(error).filter(([key]) => key !== "stack")) } };
        }
    }
    if (request.op === "create") {
        const session = manager.SessionManager.create(request.dir, request.dir);
        session.appendMessage({ role: "toolResult", toolCallId: "fixture", toolName: "fixture",
            content: [{ type: "text", text: request.text }], isError: false, timestamp: 1 });
        session.persistForRecovery();
        // Historical oversized input: serialize the source's uncapped runtime records.
        const entries = [session.getHeader(), ...session.getEntries()];
        writeFileSync(session.getSessionFile(), entries.map((entry: unknown) => JSON.stringify(entry)).join("\n") + "\n");
        return { path: session.getSessionFile() };
    }
    return tools.migrateToolResultSessionFiles(request.dir, () => {
        if (request.path !== undefined && request.advanceMtime !== undefined) {
            utimesSync(request.path, request.advanceMtime, request.advanceMtime);
        }
        return new Date(1792584000000);
    });
}
for await (const line of createInterface({ input: process.stdin })) {
    const request: Request = JSON.parse(line);
    console.log(JSON.stringify({ id: request.id, ok: true, output: runRequest(request) }));
}
