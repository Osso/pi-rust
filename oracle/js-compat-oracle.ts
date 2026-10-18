import { StringDecoder } from "node:string_decoder";

type RequestId = string | number | null;

type LocaleCompareRequest = {
	id: RequestId;
	operation: "locale_compare";
	locale: string | null;
	pairs: Array<[string, string]>;
};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { results: number[] };
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

function sign(value: number): number {
	return value < 0 ? -1 : value > 0 ? 1 : 0;
}

function execute(request: LocaleCompareRequest): Response {
	const compare =
		request.locale === null
			? (left: string, right: string) => left.localeCompare(right)
			: (left: string, right: string) => new Intl.Collator(request.locale as string).compare(left, right);
	const results = request.pairs.map(([left, right]) => sign(compare(left, right)));
	return { id: request.id, ok: true, output: { results } };
}

function executeRequestLine(line: string): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as LocaleCompareRequest;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed));
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

serveRequests();
