import { createInterface } from "node:readline";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const source = await import(pathToFileURL(join(process.argv[2], "mailbox-retention.ts")).href);
type Request = { id: number; createdAt?: unknown; now?: number; nowKind?: string };
for await (const line of createInterface({ input: process.stdin })) {
    const request: Request = JSON.parse(line);
    const special: Record<string, number> = { nan: Number.NaN, infinity: Number.POSITIVE_INFINITY, negativeInfinity: Number.NEGATIVE_INFINITY };
    const now = request.nowKind === undefined ? request.now : special[request.nowKind];
    console.log(JSON.stringify({ id: request.id, ok: true, output: {
        retention: source.MAILBOX_MESSAGE_RETENTION_MS,
        expired: source.isMailboxMessageExpired(request.createdAt, now),
    } }));
}
