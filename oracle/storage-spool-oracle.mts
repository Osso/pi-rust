import { createInterface } from "node:readline";
import { existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const spool = await import(pathToFileURL(join(process.argv[2], "session-transcript-spool.ts")).href);
type Request = { id: number; path: string; kind: string };
function observeSpool(request: Request): unknown {
    let temporary: string | undefined;
    try {
        const output = spool.withSeekableSessionTranscript(request.path, (plain: string) => {
            temporary = plain;
            if (request.kind === "callback") throw Object.assign(new TypeError("callback failed"), { code: "FIXTURE" });
            if (request.kind === "null") throw null;
            return { bytes: [...readFileSync(plain)], mode: statSync(plain).mode & 0o777 };
        });
        return { ok: true, output, temporaryExists: temporary !== undefined && existsSync(temporary) };
    } catch (error) {
        const projected = error instanceof Error
            ? { name: error.name, message: error.message, ...Object.fromEntries(Object.entries(error).filter(([key]) => key !== "stack")) }
            : error;
        return { ok: false, error: projected, temporaryExists: temporary !== undefined && existsSync(temporary) };
    }
}
for await (const line of createInterface({ input: process.stdin })) {
    const request: Request = JSON.parse(line);
    console.log(JSON.stringify({ id: request.id, ok: true, output: observeSpool(request) }));
}
