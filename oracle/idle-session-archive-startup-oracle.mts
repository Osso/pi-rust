import {
	existsSync,
	mkdirSync,
	readFileSync,
	utimesSync,
	writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { zstdDecompressSync } from "node:zlib";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const { SessionManager } = await import(
	pathToFileURL(`${source}/core/session-manager.ts`).href
);
const { createAgentSessionRuntime } = await import(
	pathToFileURL(`${source}/core/agent-session-runtime.ts`).href
);
const { readSessionMetadata } = await import(
	pathToFileURL(`${source}/core/session-control-db.ts`).href
);

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as { root: string; content: string };
		const cwd = join(request.root, "work");
		const agentDir = join(request.root, "agent");
		const file = join(agentDir, "sessions", "old-project", "old.jsonl");
		mkdirSync(cwd, { recursive: true });
		mkdirSync(join(agentDir, "sessions", "old-project"), { recursive: true });
		writeFileSync(file, request.content);
		utimesSync(file, 1600000000, 1600000000);
		const db = join(request.root, "control.db");
		const manager = SessionManager.create(cwd, join(request.root, "current"));
		manager.setMetadataControlDbPath(db);
		let factoryError: string | undefined;
		try {
			await createAgentSessionRuntime(
				async () => {
					throw new Error("fixture factory stop");
				},
				{ cwd, agentDir, sessionManager: manager },
			);
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			factoryError = error.message;
		}
		const archive = `${file}.zst`;
		const deadline = Date.now() + 5000;
		while ((!existsSync(archive) || existsSync(file)) && Date.now() < deadline)
			await new Promise((resolve) => setTimeout(resolve, 10));
		const metadata = readSessionMetadata(db, archive);
		console.log(
			JSON.stringify({
				ok: true,
				factoryError,
				archived: existsSync(archive) && !existsSync(file),
				archivedMetadata: metadata?.isArchived === true,
				content: existsSync(archive)
					? zstdDecompressSync(readFileSync(archive)).toString("utf8")
					: null,
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
