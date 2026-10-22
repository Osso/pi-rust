import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Request = { model: unknown; body: string; status?: number; apiKey: string; callback?: "payload" | "response"; units?: number[] };
type Provider = { stream(model: unknown, context: unknown, options: unknown): AsyncIterable<unknown> & { result(): Promise<unknown> } };

async function capture(provider: Provider, request: Request): Promise<string> {
    globalThis.fetch = async () => new Response(request.body, {
        status: request.status ?? 200,
        headers: { "content-type": request.status ? "application/json" : "text/event-stream" },
    });
    const fail = () => { throw new Error(String.fromCharCode(...(request.units ?? []))); };
    const stream = provider.stream(request.model, { messages: [] }, {
        apiKey: request.apiKey,
        maxRetries: 0,
        transport: "sse",
        onPayload: request.callback === "payload" ? fail : undefined,
        onResponse: request.callback === "response" ? fail : undefined,
    });
    const events = [];
    for await (const event of stream) events.push(event);
    return JSON.stringify({ events, result: await stream.result() });
}

async function main(): Promise<void> {
    const provider = await import(pathToFileURL(process.argv[2]).href) as Provider;
    for await (const line of createInterface({ input: process.stdin })) {
        const wire = await capture(provider, JSON.parse(line) as Request);
        process.stdout.write(`${JSON.stringify({ wire })}\n`);
    }
}

main().catch((error: unknown) => { console.error(error); process.exitCode = 1; });
