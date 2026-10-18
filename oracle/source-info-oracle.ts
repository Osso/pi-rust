import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Request = {
	id: RequestId;
	operation: string;
	cases?: SourceInfoCaseRequest[];
};

type OptionalState = "absent" | "present";

type ScopeState = "absent" | SourceScope;
type OriginState = "absent" | SourceOrigin;

type SourceInfoCaseRequest = {
	name: string;
	path: string;
	source: string;
	scopeState: ScopeState;
	originState: OriginState;
	baseDirState: OptionalState;
	baseDir: string;
};

type SourceScope = "user" | "project" | "temporary";
type SourceOrigin = "package" | "top-level";

type SourceInfo = {
	path: string;
	source: string;
	scope: SourceScope;
	origin: SourceOrigin;
	baseDir?: string;
};

type SyntheticSourceInfoOptions = {
	source: string;
	scope?: SourceScope;
	origin?: SourceOrigin;
	baseDir?: string;
};

type PathMetadata = {
	source: string;
	scope: SourceScope;
	origin: SourceOrigin;
	baseDir?: string;
};

type PathMetadataCaseRequest = {
	name: string;
	path: string;
	source: string;
	scope: SourceScope;
	origin: SourceOrigin;
	baseDirState: OptionalState;
	baseDir: string;
};

type SourceInfoApi = {
	createSyntheticSourceInfo(path: string, options: SyntheticSourceInfoOptions): SourceInfo;
	createSourceInfo(path: string, metadata: PathMetadata): SourceInfo;
};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: unknown;
	  }
	| {
			id: RequestId;
			ok: false;
			error: { name: string; message: string };
	  };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: SourceInfoApi): Response {
	if (request.operation === "source_info" && request.cases !== undefined) {
		const results = request.cases.map((input) => normalizeCase(input, api));
		return { id: request.id, ok: true, output: { results } };
	}
	if (request.operation === "create_source_info") {
		const cases = (request as { metadataCases?: PathMetadataCaseRequest[] }).metadataCases ?? [];
		const results = cases.map((input) => normalizeMetadataCase(input, api));
		return { id: request.id, ok: true, output: { results } };
	}
	throw new Error(`unsupported operation: ${request.operation}`);
}

function normalizeMetadataCase(input: PathMetadataCaseRequest, api: SourceInfoApi): unknown {
	const metadata: PathMetadata = {
		source: input.source,
		scope: input.scope,
		origin: input.origin,
	};
	if (input.baseDirState === "present") metadata.baseDir = input.baseDir;
	const result = api.createSourceInfo(input.path, metadata);
	return {
		name: input.name,
		result: normalizeSourceInfo(result),
		metadataAfter: normalizeMetadata(metadata),
	};
}

function normalizeMetadata(metadata: PathMetadata): Record<string, unknown> {
	const value: Record<string, unknown> = {
		source: metadata.source,
		scope: metadata.scope,
		origin: metadata.origin,
	};
	if (metadata.baseDir !== undefined) value.baseDir = metadata.baseDir;
	return value;
}

function normalizeCase(input: SourceInfoCaseRequest, api: SourceInfoApi): unknown {
	const options: SyntheticSourceInfoOptions = { source: input.source };
	if (input.scopeState !== "absent") options.scope = input.scopeState;
	if (input.originState !== "absent") options.origin = input.originState;
	if (input.baseDirState === "present") options.baseDir = input.baseDir;
	const result = api.createSyntheticSourceInfo(input.path, options);
	return {
		name: input.name,
		result: normalizeSourceInfo(result),
		optionsAfter: normalizeOptions(options),
	};
}

function normalizeSourceInfo(result: SourceInfo): Record<string, unknown> {
	const value: Record<string, unknown> = {
		path: result.path,
		source: result.source,
		scope: result.scope,
		origin: result.origin,
	};
	if (result.baseDir !== undefined) value.baseDir = result.baseDir;
	return value;
}

function normalizeOptions(options: SyntheticSourceInfoOptions): Record<string, unknown> {
	const value: Record<string, unknown> = { source: options.source };
	if (options.scope !== undefined) value.scope = options.scope;
	if (options.origin !== undefined) value.origin = options.origin;
	if (options.baseDir !== undefined) value.baseDir = options.baseDir;
	return value;
}

function executeRequestLine(line: string, api: SourceInfoApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: SourceInfoApi): void {
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
		throw new Error("source info oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as SourceInfoApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`source info oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
