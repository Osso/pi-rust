// Frozen modules only. Fixtures use private SQLite files and in-memory streams, never SSH/network.
import { createInterface } from "node:readline";
import { PassThrough, Writable } from "node:stream";
import { setTimeout as delay } from "node:timers/promises";
import { pathToFileURL } from "node:url";
async function main(): Promise<void> {
const root = process.argv[2];
const store = await import(pathToFileURL(`${root}/core/host-relay-store.ts`).href);
const db = await import(pathToFileURL(`${root}/core/session-control-db.ts`).href);
const relay = await import(pathToFileURL(`${root}/core/host-relay.ts`).href);
for await (const line of createInterface({ input: process.stdin })) {
    try {
        const request = JSON.parse(line);
        let result: unknown;
        if (request.op === "protocol") {
            const input = new PassThrough();
            let text = "";
            let released = !request.blocked;
            const pending: (() => void)[] = [];
            const output = new Writable({ highWaterMark: 1, write(chunk, _encoding, callback) {
                text += chunk.toString();
                if (released) callback(); else pending.push(callback);
            } });
            const done = relay.runHostRelay({ controlDbPath: request.path, host: "fixture-local", input, output });
            for (const frame of request.frames) input.write(frame + "\n");
            let importedWhileBlocked: number | undefined;
            if (request.blocked) {
                await delay(20);
                importedWhileBlocked = db.listRuntimeMailboxMessages(request.path).length;
                released = true;
                for (const callback of pending) callback();
            }
            input.end();
            let error: string | null = null;
            try { await done; } catch (failure) { error = (failure as Error).message; }
            result = { frames: text.trim().split("\n").map(value => JSON.parse(value)), error, ...(request.blocked ? { importedWhileBlocked } : {}) };
        } else if (request.op === "raw") {
            result = db.withControlDb(request.path, (sql: any) => sql.prepare(request.sql).all());
        } else {
            const api = request.op === "db" ? db : store;
            result = await api[request.fn](...JSON.parse(request.args));
        }
        process.stdout.write(JSON.stringify({ ok: true, wire: JSON.stringify(result ?? null) }) + "\n");
    } catch (failure) {
        process.stdout.write(JSON.stringify({ ok: false, error: (failure as Error).message }) + "\n");
    }
}

}
main().catch(error => { console.error(error); process.exitCode = 1; });
