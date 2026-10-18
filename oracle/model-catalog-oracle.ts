import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type Request = { id: number; groups: Array<Record<string, unknown>> };

type CatalogApi = {
	flattenModelCatalog(provider: string, groups: Record<string, Record<string, unknown>>): Record<string, unknown>;
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: CatalogApi) {
	try {
		const groups = Object.assign({}, ...request.groups.map((group, index) => ({ [`group-${index}`]: group })));
		return { id: request.id, ok: true, output: { catalog: api.flattenModelCatalog("test-provider", groups) } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function serve(api: CatalogApi): void {
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
	if (modulePath === undefined) throw new Error("model catalog oracle requires a source module path");
	const api = (await import(pathToFileURL(modulePath).href)) as CatalogApi;
	serve(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`model catalog oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
