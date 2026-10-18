import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = { id: number; messages: unknown[]; hasImages: boolean };

type CopilotApi = {
	inferCopilotInitiator(messages: unknown[]): "user" | "agent";
	hasCopilotVisionInput(messages: unknown[]): boolean;
	buildCopilotDynamicHeaders(params: { messages: unknown[]; hasImages: boolean }): Record<string, string>;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: CopilotApi) {
	try {
		return {
			id: request.id,
			ok: true,
			output: {
				initiator: api.inferCopilotInitiator(request.messages),
				hasImages: api.hasCopilotVisionInput(request.messages),
				headers: api.buildCopilotDynamicHeaders({
					messages: request.messages,
					hasImages: request.hasImages,
				}),
			},
		};
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: CopilotApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		for (;;) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			process.stdout.write(`${JSON.stringify(execute(JSON.parse(buffer.slice(0, lineFeed)), api))}\n`);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("github copilot headers oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as CopilotApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`github copilot headers oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
