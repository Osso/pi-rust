// Frozen modules only. Private SQLite/streams/executable SSH fakes, never real hosts.
import { createInterface } from "node:readline";
import { PassThrough, Writable } from "node:stream";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { setTimeout as delay } from "node:timers/promises";
import { pathToFileURL } from "node:url";

type Request = { op: string; fn: string; args: string; path: string; frames: string[]; blocked?: boolean;
    settings?: { peer: string; remoteCommand?: string }; readyPath?: string; stopAfterMs?: number };

async function loadApi(root: string) {
    const load = (file: string) => import(pathToFileURL(`${root}/${file}.ts`).href);
    return { store: await load("core/host-relay-store"), db: await load("core/session-control-db"),
        relay: await load("core/host-relay"), client: await load("core/host-relay-client"),
        resident: await load("core/host-relay-resident"), process: await load("core/runtime-process"),
        config: await load("config") };
}
type Api = Awaited<ReturnType<typeof loadApi>>;

async function runProtocol(api: Api, request: Request): Promise<unknown> {
    const input = new PassThrough();
    let text = "";
    let released = !request.blocked;
    const pending: (() => void)[] = [];
    const output = new Writable({ highWaterMark: 1, write(chunk, _encoding, callback) {
        text += chunk.toString();
        if (released) callback(); else pending.push(callback);
    } });
    const done = api.relay.runHostRelay({ controlDbPath: request.path, host: "fixture-local", input, output });
    for (const frame of request.frames) input.write(frame + "\n");
    let importedWhileBlocked: number | undefined;
    if (request.blocked) {
        await delay(20);
        importedWhileBlocked = api.db.listRuntimeMailboxMessages(request.path).length;
        released = true;
        for (const callback of pending) callback();
    }
    input.end();
    let error: string | null = null;
    try { await done; } catch (failure) { error = (failure as Error).message; }
    return { frames: text.trim().split("\n").map(value => JSON.parse(value)), error,
        ...(request.blocked ? { importedWhileBlocked } : {}) };
}

async function runClient(api: Api, request: Request): Promise<unknown> {
    const controller = new AbortController();
    const logs: string[] = [];
    const original = console.error;
    console.error = (...values: unknown[]) => { logs.push(values.join(" ")); };
    const task = api.client.runSshRelayClient(request.path, request.settings, controller.signal);
    const deadline = Date.now() + 8000;
    try {
        if (request.stopAfterMs !== undefined) {
            await delay(request.stopAfterMs);
            return logs;
        }
        while (Date.now() < deadline) {
            if (request.readyPath && existsSync(request.readyPath) && api.db.listRuntimeMailboxMessages(request.path).length === 0) break;
            await delay(10);
        }
        if (Date.now() >= deadline) throw new Error("fake SSH deadline exceeded");
    } finally {
        controller.abort();
        await task;
        console.error = original;
    }
    return logs;
}

async function execute(api: Api, request: Request): Promise<unknown> {
    if (request.op === "protocol") return runProtocol(api, request);
    if (request.op === "client") return runClient(api, request);
    if (request.op === "identity") {
        return { ...api.process.readProcessIdentity(process.pid), version: api.config.VERSION };
    }
    if (request.op === "ensure") {
        await api.resident.ensureRelayRunning(request.path, request.settings);
        return readFileSync(`${request.path}.relay.json`, "utf8");
    }
    if (request.op === "writeIdentity") {
        const wire = JSON.stringify({ ...api.process.readProcessIdentity(process.pid), version: api.config.VERSION });
        writeFileSync(`${request.path}.relay.json`, wire, { mode: 0o600 });
        return wire;
    }
    if (request.op === "raw") {
        return api.db.withControlDb(request.path, (sql: { prepare(query: string): { all(): unknown } }) => sql.prepare(request.args).all());
    }
    const target = request.op === "db" ? api.db : api.store;
    return Reflect.apply(target[request.fn], undefined, JSON.parse(request.args));
}

async function main(): Promise<void> {
    const root = process.argv[2];
    if (!root) throw new Error("host relay oracle requires frozen coding-agent src");
    const api = await loadApi(root);
    for await (const line of createInterface({ input: process.stdin })) {
        try {
            const result = await execute(api, JSON.parse(line));
            process.stdout.write(JSON.stringify({ ok: true, wire: JSON.stringify(result ?? null) }) + "\n");
        } catch (failure) {
            process.stdout.write(JSON.stringify({ ok: false, error: (failure as Error).message }) + "\n");
        }
    }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
