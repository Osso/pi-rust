import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ParsedSkillBlock = {
	name: string;
	location: string;
	content: string;
	userMessage: string | undefined;
};

type SkillBlockApi = {
	parseSkillBlock(text: string): ParsedSkillBlock | null;
};

type SkillBlockCase = {
	name: string;
	text: string;
};

type SkillBlockRequest = {
	id: RequestId;
	operation: "parse_skill_block";
	cases: SkillBlockCase[];
};

type NormalizedSkillBlock = {
	name: string;
	location: string;
	content: string;
	userMessage: string | null;
};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { results: Array<{ name: string; parsed: NormalizedSkillBlock | null }> };
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

function normalizeSkillBlock(block: ParsedSkillBlock | null): NormalizedSkillBlock | null {
	if (block === null) return null;
	return {
		name: block.name,
		location: block.location,
		content: block.content,
		userMessage: block.userMessage ?? null,
	};
}

function execute(request: SkillBlockRequest, api: SkillBlockApi): Response {
	try {
		const results = request.cases.map((skillBlockCase) => ({
			name: skillBlockCase.name,
			parsed: normalizeSkillBlock(api.parseSkillBlock(skillBlockCase.text)),
		}));
		return { id: request.id, ok: true, output: { results } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function executeRequestLine(line: string, api: SkillBlockApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as SkillBlockRequest;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: SkillBlockApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("skill block oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as SkillBlockApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`skill block oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
