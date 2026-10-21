import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = { id: number; message: unknown };
type RetryApi = {
	isRetryableAssistantError(message: unknown): boolean;
	isCodexCybersecurityRestriction(message: unknown): boolean;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error)
		return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function execute(request: Request, api: RetryApi) {
	try {
		return {
			id: request.id,
			ok: true,
			output: {
				retryable: api.isRetryableAssistantError(request.message),
				cybersecurity: api.isCodexCybersecurityRestriction(request.message),
			},
		};
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
	if (modulePath === undefined)
		throw new Error("retry oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as RetryApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`retry oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
