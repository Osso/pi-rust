// Real frozen backend process; never blocks event loop during the held callback.
import { pathToFileURL } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { readFileSync, existsSync } from "node:fs";
async function main(): Promise<void> {
const [, , sourceRoot, mode, file, milliseconds] = process.argv;
const api = await import(pathToFileURL(`${sourceRoot}/packages/coding-agent/src/core/auth-storage.ts`).href);
const backend = new api.FileAuthStorageBackend(file);
if (mode === "hold") {
	await backend.withLockAsync(async () => {
		process.stdout.write("LOCKED\n");
		const deadline = Date.now() + Number(milliseconds);
		while (!existsSync(`${file}.release`) && Date.now() < deadline) await sleep(20);
		return { result: undefined };
	});
	process.stdout.write("RELEASED\n");
} else if (mode === "try-sync") {
	try { backend.withLock(() => ({ result: undefined })); process.stdout.write("ACQUIRED\n"); }
	catch (error) { process.stdout.write(`${(error as {code?: string}).code}\n`); }
} else if (mode === "read") {
	const storage = api.AuthStorage.create(file);
	process.stdout.write(`${JSON.stringify({ data: storage.getAll(), errors: storage.drainErrors().map((e: Error) => e.message) })}\n`);
} else {
	throw new Error(`Unknown mode ${mode}: ${readFileSync(file, "utf8").length}`);
}
}
void main().catch(error => { process.stderr.write(`${String(error)}\n`); process.exitCode = 1; });
