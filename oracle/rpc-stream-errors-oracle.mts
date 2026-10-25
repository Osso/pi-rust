import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { PassThrough } from "node:stream";
import { pathToFileURL } from "node:url";

type JsonlApi = {
	attachJsonlLineReader(
		stream: PassThrough,
		onLine: (line: string) => void,
	): () => void;
};
type Client = { start(): Promise<void>; stop(): Promise<void> };
type ClientApi = { RpcClient: new (options: { cliPath: string }) => Client };
type Request = { operation: "read" | "callback" | "stderr" };

function errorRecord(error: Error): Record<string, unknown> {
	const value: Record<string, unknown> = {
		name: error.name,
		message: error.message,
	};
	for (const field of ["code", "errno", "syscall"])
		if (field in error) value[field] = Reflect.get(error, field);
	return value;
}
function readError(): Error {
	return Object.assign(new Error("read EIO"), {
		errno: -5,
		code: "EIO",
		syscall: "read",
	});
}
async function captureUncaught(fire: () => void): Promise<unknown> {
	const caught = new Promise<Error>((resolve) =>
		process.once("uncaughtException", resolve),
	);
	setImmediate(fire);
	return errorRecord(await caught);
}
async function jsonl(
	api: JsonlApi,
	operation: "read" | "callback",
): Promise<unknown> {
	const stream = new PassThrough();
	const detach = api.attachJsonlLineReader(stream, () => {
		throw Object.assign(new TypeError("callback-\ud800"), { code: "ECALL" });
	});
	try {
		return await captureUncaught(() => {
			if (operation === "read") stream.destroy(readError());
			else stream.write("line\n");
		});
	} finally {
		detach();
		stream.destroy();
	}
}
async function stderr(api: ClientApi): Promise<unknown> {
	const root = await mkdtemp(join(tmpdir(), "rpc-stderr-"));
	const path = join(root, "child.mjs");
	await writeFile(path, "process.stdin.resume();setInterval(()=>{},1000);");
	const client = new api.RpcClient({ cliPath: path });
	try {
		await client.start();
		const child = Reflect.get(client, "process") as { stderr: PassThrough };
		return await captureUncaught(() => child.stderr.destroy(readError()));
	} finally {
		await client.stop();
		await rm(root, { recursive: true, force: true });
	}
}
const api = await import(pathToFileURL(process.argv[2]).href);
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as Request;
	const result =
		request.operation === "stderr"
			? await stderr(api as ClientApi)
			: await jsonl(api as JsonlApi, request.operation);
	process.stdout.write(JSON.stringify(result) + "\n");
}
