import { createInterface } from "node:readline";
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

const spool = await import(pathToFileURL(join(process.argv[2], "session-transcript-spool.ts")).href);
type Request = { id: number; path: string; kind: string; tmpdir?: string; op?: string };
function observeSpool(request: Request): unknown {
    let temporary: string | undefined;
    try {
        if (request.op === "mkdtemp") {
            const path = mkdtempSync(request.path);
            return { ok: true, path, mode: statSync(path).mode & 0o777 };
        }
        const output = spool.withSeekableSessionTranscript(request.path, (plain: string) => {
            temporary = plain;
            if (request.kind === "path") return { path: plain };
            if (request.kind === "remove") {
                const bytes = [...readFileSync(plain)];
                rmSync(dirname(plain), { recursive: true, force: true });
                return { bytes };
            }
            if (request.kind === "cleanup") {
                chmodSync(dirname(plain), 0);
                throw new Error("callback failure must be replaced by cleanup failure");
            }
            if (request.kind === "callback") throw Object.assign(new TypeError("callback failed"), { code: "FIXTURE" });
            if (request.kind === "null") throw null;
            return { bytes: [...readFileSync(plain)], mode: statSync(plain).mode & 0o777 };
        });
        return { ok: true, output, temporaryExists: temporary !== undefined && existsSync(temporary) };
    } catch (error) {
        const projected = error instanceof Error
            ? { name: error.name, message: error.message, ...Object.fromEntries(Object.entries(error).filter(([key]) => key !== "stack")) }
            : error;
        if (request.kind === "cleanup" && temporary !== undefined) {
            const directory = dirname(temporary);
            chmodSync(directory, 0o700);
            rmSync(directory, { recursive: true, force: true });
            return { ok: false, error: projected, directory };
        }
        return { ok: false, error: projected, temporaryExists: temporary !== undefined && existsSync(temporary) };
    }
}
for await (const line of createInterface({ input: process.stdin })) {
    const request: Request = JSON.parse(line);
    const previous = process.env.TMPDIR;
    try {
        if (request.tmpdir !== undefined) process.env.TMPDIR = request.tmpdir;
        console.log(JSON.stringify({ id: request.id, ok: true, output: observeSpool(request) }));
    } finally {
        if (previous === undefined) delete process.env.TMPDIR;
        else process.env.TMPDIR = previous;
    }
}
