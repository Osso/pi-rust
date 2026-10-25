import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Writer = { appendProgress(update: unknown): void; finish(record: unknown): void };
runOracle((api, request) => requireCases(request).map(value => {
	const input = value as { count: number; size: number; terminalSize: number };
	const directory = mkdtempSync(join(tmpdir(), "pyrun-writer-"));
	const path = join(directory, "output");
	try {
		const writer = new (api.PyrunArtifactWriter as new (path: string) => Writer)(path);
		for (let index = 0; index < input.count; index++) writer.appendProgress({ type: "console", text: "x".repeat(input.size), index });
		let head = Buffer.alloc(0);
		try { head = readFileSync(path); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error; }
		writer.finish({ kind: "error", error: "e".repeat(input.terminalSize) });
		const bytes = readFileSync(path);
		return { headBytes: head.length, bytes: bytes.length, hash: createHash("sha256").update(bytes).digest("hex"), appendOnly: bytes.subarray(0, head.length).equals(head) };
	} finally { rmSync(directory, { recursive: true }); }
})).catch(error => { console.error(error); process.exitCode = 1; });
