import { createInterface } from "node:readline";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { writeFileSync } from "node:fs";
import { zstdCompressSync } from "node:zlib";

const source = await import(pathToFileURL(join(process.argv[2], "session-transcript.ts")).href);

async function observeFailure(path: string, mode: string): Promise<unknown> {
    try {
        if (mode === "bytes") source.readSessionTranscriptBytes(path);
        else if (mode === "text") source.readSessionTranscriptText(path);
        else {
            for await (const _chunk of source.createSessionTranscriptReadStream(path)) {
                // Consume the actual public stream, including its asynchronous failure.
            }
        }
        return { ok: true };
    } catch (error) {
        if (!(error instanceof Error)) throw error;
        const own = Object.fromEntries(Object.entries(error).filter(([key]) => key !== "stack"));
        return {
            ok: false,
            error: { name: error.name, message: error.message, ...own },
            stringValue: String(error),
            constructor: error instanceof TypeError ? "TypeError" : "Error",
        };
    }
}

for await (const line of createInterface({ input: process.stdin })) {
    const request: { id: number; path: string; mode: string; op?: string; bytes?: number[] } = JSON.parse(line);
    let output: unknown;
    if (request.op === "write") {
        if (!request.bytes) throw new Error("write requires fixture bytes");
        writeFileSync(request.path, zstdCompressSync(Buffer.from(request.bytes)));
        output = { written: true };
    } else if (request.op === "read") {
        let streamed = "";
        for await (const chunk of source.createSessionTranscriptReadStream(request.path)) streamed += chunk;
        output = {
            bytes: [...source.readSessionTranscriptBytes(request.path)],
            text: source.readSessionTranscriptText(request.path),
            streamed,
            resolved: source.readSessionTranscriptPath(request.path),
        };
    } else {
        output = await observeFailure(request.path, request.mode);
    }
    console.log(JSON.stringify({ id: request.id, ok: true, output }));
}
