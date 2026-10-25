import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const source = process.argv[2];
if (!source) throw new Error("device code source required");
const { pollOAuthDeviceCodeFlow } = await import(pathToFileURL(source).href);
const realNow = Date.now;
const realTimeout = globalThis.setTimeout;
for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    let now = 0;
    const polls: number[] = [];
    const controller = new AbortController();
    if (request.abort) controller.abort("caller reason is deliberately ignored");
    Date.now = () => now;
    globalThis.setTimeout = ((callback: () => void, delay: number) => {
        queueMicrotask(() => { now += delay; callback(); });
        return 1;
    }) as unknown as typeof setTimeout;
    let outcome;
    try {
        let index = 0;
        const value = await pollOAuthDeviceCodeFlow({
            ...request.options,
            signal: controller.signal,
            poll: async () => {
                polls.push(now);
                if (request.reject) throw new TypeError("fetch failed");
                return request.steps[index++] ?? { status: "complete", value: "token" };
            }
        });
        outcome = { ok: true, value };
    } catch (error) {
        if (!(error instanceof Error)) throw error;
        outcome = { ok: false, name: error.name, message: error.message };
    } finally {
        Date.now = realNow;
        globalThis.setTimeout = realTimeout;
    }
    process.stdout.write(`${JSON.stringify({ polls, elapsed: now, outcome })}\n`);
}
