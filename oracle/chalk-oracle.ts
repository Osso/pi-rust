import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type StyleCase = {
	name: string;
	level: number;
	styles: string[];
	text: string;
};

type ChalkRequest = {
	id: RequestId;
	operation: "style";
	cases: StyleCase[];
};

type ChalkNamespace = {
	Chalk: new (options: { level: number }) => Record<string, unknown>;
};

type ChalkBuilder = ((text: string) => string) & Record<string, ChalkBuilder>;

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { results: Array<{ name: string; value: string }> };
	  }
	| {
			id: RequestId;
			ok: false;
			error: { name: string; message: string };
	  };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function applyCase(namespace: ChalkNamespace, styleCase: StyleCase): string {
	let builder = new namespace.Chalk({ level: styleCase.level }) as unknown as ChalkBuilder;
	for (const style of styleCase.styles) {
		builder = builder[style];
	}
	return builder(styleCase.text);
}

function execute(request: ChalkRequest, namespace: ChalkNamespace): Response {
	try {
		const results = request.cases.map((styleCase) => ({
			name: styleCase.name,
			value: applyCase(namespace, styleCase),
		}));
		return { id: request.id, ok: true, output: { results } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, namespace: ChalkNamespace): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as ChalkRequest;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, namespace))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(namespace: ChalkNamespace): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), namespace);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readChalkModulePath(): string {
	const modulePath = process.argv[2];
	if (modulePath === undefined) {
		throw new Error("chalk oracle requires a chalk module path");
	}
	return modulePath;
}

async function main(): Promise<void> {
	const namespace = (await import(pathToFileURL(readChalkModulePath()).href)) as ChalkNamespace;
	serveRequests(namespace);
}

void main().catch((error: unknown) => {
	const response: Response = { id: null, ok: false, error: normalizeError(error) };
	process.stdout.write(`${JSON.stringify(response)}\n`);
	process.exitCode = 1;
});
