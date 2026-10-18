import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RetryPolicy = { enabled: boolean; maxRetries: number; baseDelayMs: number };

type Request =
	| { id: number; operation: "classify"; message: unknown }
	| { id: number; operation: "retry"; policy: RetryPolicy | null; responses: unknown[] };

type RetryApi = {
	isRetryableAssistantError(message: unknown): boolean;
	retryAssistantCall(
		produce: () => Promise<unknown>,
		policy: RetryPolicy | undefined,
		signal: AbortSignal | undefined,
		callbacks?: {
			onRetryScheduled?: (attempt: number, maxAttempts: number, delayMs: number, errorMessage: string) => void;
			onRetryAttemptStart?: () => void;
			onRetryFinished?: (success: boolean, attempt: number, finalError?: string) => void;
		},
	): Promise<unknown>;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function execute(request: Request, api: RetryApi) {
	try {
		if (request.operation === "classify") {
			return { id: request.id, ok: true, output: { retryable: api.isRetryableAssistantError(request.message) } };
		}
		const events: Array<Record<string, unknown>> = [];
		let index = 0;
		const produce = async () => {
			const response = request.responses[Math.min(index, request.responses.length - 1)];
			index++;
			return response;
		};
		const final = await api.retryAssistantCall(produce, request.policy ?? undefined, undefined, {
			onRetryScheduled: (attempt, maxAttempts, delayMs, errorMessage) => {
				events.push({ kind: "scheduled", attempt, maxAttempts, delayMs, errorMessage });
			},
			onRetryAttemptStart: () => {
				events.push({ kind: "attempt-start" });
			},
			onRetryFinished: (success, attempt, finalError) => {
				events.push({ kind: "finished", success, attempt, finalError: finalError ?? null });
			},
		});
		return { id: request.id, ok: true, output: { final, events, calls: index } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: RetryApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const request = JSON.parse(buffer.slice(0, lineFeed)) as Request;
			buffer = buffer.slice(lineFeed + 1);
			void execute(request, api).then((response) => {
				process.stdout.write(`${JSON.stringify(response)}\n`);
			});
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("retry oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as RetryApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`retry oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
