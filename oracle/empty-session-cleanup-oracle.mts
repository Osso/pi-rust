import { existsSync, mkdirSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { SessionManager } = await import(
	pathToFileURL(`${root}/core/session-manager.ts`).href
);
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${root}/core/agent-session-runtime.ts`).href
);
const { readSessionMetadata } = await import(
	pathToFileURL(`${root}/core/session-control-db.ts`).href
);

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as {
			file: string;
			db: string;
			cwd: string;
			directory?: boolean;
		};
		writeFileSync(
			request.file,
			`${JSON.stringify({ type: "session", version: 3, id: "cleanup-fixture", cwd: request.cwd, timestamp: "2026-10-23T00:00:00.000Z" })}\n`,
		);
		const manager = SessionManager.open(request.file, dirname(request.file));
		manager.setMetadataControlDbPath(request.db);
		let disposed = false;
		const runtime = Object.create(AgentSessionRuntime.prototype);
		runtime.lifecycleTransitionTail = Promise.resolve();
		runtime._session = {
			sessionManager: manager,
			extensionRunner: { hasHandlers: () => false },
			dispose() {
				disposed = true;
			},
		};
		if (request.directory) {
			unlinkSync(request.file);
			mkdirSync(request.file);
		}
		let failure: object | undefined;
		try {
			await runtime.dispose();
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			const record = error as Error & {
				code?: string;
				errno?: number;
				syscall?: string;
				path?: string;
			};
			const units = (value: string) =>
				Array.from({ length: value.length }, (_, index) =>
					value.charCodeAt(index),
				);
			failure = {
				name: error.name,
				message: units(error.message),
				code: record.code,
				errno: record.errno,
				syscall: record.syscall,
				path: record.path === undefined ? null : units(record.path),
			};
		}
		console.log(
			JSON.stringify({
				ok: failure === undefined,
				...(failure === undefined ? {} : { error: failure }),
				fileExists: existsSync(request.file),
				metadataExists:
					readSessionMetadata(request.db, request.file) !== undefined,
				disposed,
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
