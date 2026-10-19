import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type TestArtifact = {
	type: string;
	runId?: string;
	attachments?: Array<{ name: string; contentType: string; body: string; bodyEncoding: string }>;
};

type ArtifactsApi = {
	persistEvalArtifactReferences(
		artifacts: ReadonlyArray<TestArtifact>,
		runId: string,
		artifactDirectory: string,
	): Promise<Array<{ name: string; path: string }>>;
};

type ArtifactsRequest = {
	id: RequestId;
	operation: "persist";
	artifacts: TestArtifact[];
	runId: string;
};

type Response =
	| {
			id: RequestId;
			ok: true;
			output: { references: Array<{ name: string; path: string }>; contents: Array<{ path: string; body: string }> };
	  }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

async function execute(request: ArtifactsRequest, api: ArtifactsApi): Promise<Response> {
	const directory = await mkdtemp(join(tmpdir(), "pi-eval-artifacts-oracle-"));
	try {
		const references = await api.persistEvalArtifactReferences(request.artifacts, request.runId, directory);
		const contents = [];
		for (const reference of references) {
			contents.push({ path: reference.path, body: await readFile(join(directory, reference.path), "utf8") });
		}
		return { id: request.id, ok: true, output: { references, contents } };
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	} finally {
		await rm(directory, { recursive: true, force: true });
	}
}

async function executeRequestLine(line: string, api: ArtifactsApi): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as ArtifactsRequest;
		id = request.id;
		process.stdout.write(`${JSON.stringify(await execute(request, api))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: ArtifactsApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let chain = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			chain = chain.then(() => executeRequestLine(line, api));
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) throw new Error("evals artifacts oracle requires a source module path");
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as ArtifactsApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`evals artifacts oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
