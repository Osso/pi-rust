import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type LocaleCompareRequest = {
	id: RequestId;
	operation: "locale_compare";
	locale: string | null;
	pairs: Array<[string, string]>;
};

type MinimatchCase = {
	path: string;
	pattern: string;
	options: Record<string, unknown>;
};

type MinimatchRequest = {
	id: RequestId;
	operation: "minimatch";
	cases: MinimatchCase[];
};

type BraceExpandRequest = {
	id: RequestId;
	operation: "brace_expand";
	patterns: string[];
	options: Record<string, unknown>;
};

type Request = LocaleCompareRequest | MinimatchRequest | BraceExpandRequest;

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { results: Array<number | boolean | string[]> };
	  }
	| {
			id: RequestId;
			ok: false;
			error: { name: string; message: string };
	  };

type MinimatchModule = {
	minimatch(path: string, pattern: string, options?: Record<string, unknown>): boolean;
	braceExpand(pattern: string, options?: Record<string, unknown>): string[];
};

let minimatchModulePromise: Promise<MinimatchModule> | null = null;

function loadMinimatch(): Promise<MinimatchModule> {
	if (minimatchModulePromise === null) {
		const modulePath = process.argv[2];
		if (modulePath === undefined) {
			throw new Error("js-compat oracle requires a minimatch module path");
		}
		minimatchModulePromise = import(pathToFileURL(modulePath).href) as Promise<MinimatchModule>;
	}
	return minimatchModulePromise;
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function sign(value: number): number {
	return value < 0 ? -1 : value > 0 ? 1 : 0;
}

function executeLocaleCompare(request: LocaleCompareRequest): Response {
	const compare =
		request.locale === null
			? (left: string, right: string) => left.localeCompare(right)
			: (left: string, right: string) => new Intl.Collator(request.locale as string).compare(left, right);
	const results = request.pairs.map(([left, right]) => sign(compare(left, right)));
	return { id: request.id, ok: true, output: { results } };
}

async function execute(request: Request): Promise<Response> {
	if (request.operation === "locale_compare") {
		return executeLocaleCompare(request);
	}
	const minimatchModule = await loadMinimatch();
	if (request.operation === "minimatch") {
		const results = request.cases.map((testCase) =>
			minimatchModule.minimatch(testCase.path, testCase.pattern, testCase.options),
		);
		return { id: request.id, ok: true, output: { results } };
	}
	const results = request.patterns.map((pattern) =>
		minimatchModule.braceExpand(pattern, request.options),
	);
	return { id: request.id, ok: true, output: { results } };
}

async function executeRequestLine(line: string): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(await execute(request))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let chain: Promise<void> = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			chain = chain.then(() => executeRequestLine(line));
		}
	});
}

serveRequests();
