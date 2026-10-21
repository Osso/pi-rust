import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { zstdCompressSync } from "node:zlib";
import { selectApprovedModule } from "./approved-delta.mjs";

const module = selectApprovedModule(process.argv[2], "packages/coding-agent/src/core/session-transcript.ts");
const script = `
import { createSessionTranscriptReadStream } from ${JSON.stringify(pathToFileURL(module).href)};
try {
 let bytes = 0;
 for await (const chunk of createSessionTranscriptReadStream(process.argv[1])) bytes += Buffer.byteLength(chunk);
 console.log(JSON.stringify({ bytes }));
} catch (error) {
 console.log(JSON.stringify({ error: error.code }));
}
`;
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const text = "header\nUnicode café 🧭\n".repeat(100_000);
	const compressed = zstdCompressSync(text);
	writeFileSync(request.path, compressed.subarray(0, compressed.length - 1));
	// No test framework or uncaught-exception handlers in this consumer.
	const child = spawnSync(process.execPath, ["--input-type=module", "-e", script, request.path], {
		encoding: "utf8", timeout: 10_000,
	});
	if (child.error) throw child.error;
	console.log(JSON.stringify({ status: child.status, signal: child.signal, stdout: child.stdout, stderr: child.stderr }));
}
