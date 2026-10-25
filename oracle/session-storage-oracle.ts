import { readFileSync, statSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";

async function main() {
	const root = process.argv[2];
	const manager = await import(
		pathToFileURL(join(root, "session-manager.ts")).href
	);
	const archive = await import(
		pathToFileURL(join(root, "session-archive-storage.ts")).href
	);
	const transcript = await import(
		pathToFileURL(join(root, "session-transcript.ts")).href
	);
	const history = await import(
		pathToFileURL(join(root, "session-history.ts")).href
	);
	async function handle(r: any) {
		if (r.op === "create") {
			const m = manager.SessionManager.create(r.cwd, r.dir, r.options);
			if (r.db) m.setMetadataControlDbPath(r.db);
			const first = m.appendMessage(
				r.toolResult
					? {
							role: "toolResult",
							toolCallId: "source-call",
							toolName: "fixture",
							content: [{ type: "text", text: r.text ?? "tool result" }],
							isError: false,
							timestamp: 1,
						}
					: { role: "user", content: r.text ?? "hello 😀漢字", timestamp: 1 },
				r.refs,
			);
			if (r.compacted || r.branched) {
				const kept = m.appendMessage(
					{ role: "user", content: "kept", timestamp: 2 },
					r.refs,
				);
				m.branch(first);
				for (let i = 0; i < (r.inactiveCount ?? 1); i++)
					m.appendMessage({
						role: "user",
						content: r.largeInactive ? "漢😀".repeat(512 * 1024) : "inactive",
						timestamp: 3,
					});
				m.branch(kept);
				if (r.compacted) m.appendCompaction("summary", kept, 123);
				m.appendMessage({ role: "user", content: "newest", timestamp: 4 });
			}
			if (r.splitSurrogate)
				for (const entry of m.getEntries())
					if (entry.type === "message")
						entry.mailboxStoreRefs = [
							{
								sessionPath: String.fromCharCode(0xd800),
								messageId: "surrogate",
							},
						];
			if (r.splitExtra)
				for (const entry of m.getEntries())
					if (entry.type === "compaction") {
						entry[String.fromCharCode(0xd800)] = {
							value: String.fromCharCode(0xdc00),
						};
						entry.usage = {
							[String.fromCharCode(0xdc00)]: String.fromCharCode(0xd800),
						};
					}
			if ("legacyUsage" in r)
				for (const entry of m.getEntries())
					if (entry.type === "compaction") entry.usage = r.legacyUsage;
			if ("rawRefs" in r)
				for (const entry of m.getEntries())
					if (entry.type === "message") entry.mailboxStoreRefs = r.rawRefs;
			if (r.legacy1) {
				m.getHeader().version = 1;
				for (const entry of m.getEntries()) {
					delete entry.id;
					delete entry.parentId;
				}
			}
			m.persistForRecovery();
			if (r.oversizedRaw) {
				const entries = m.getEntries();
				entries.find(
					(e: any) => e.type === "message" && e.message.role === "toolResult",
				).message.content = [{ type: "text", text: '漢😀\\"'.repeat(180000) }];
				writeFileSync(
					m.getSessionFile(),
					[m.getHeader(), ...entries]
						.map((e: any) => JSON.stringify(e))
						.join("\n") + "\n",
				);
			}
			return { path: m.getSessionFile(), id: m.getSessionId() };
		}
		if (r.op === "historyAfterRemove" || r.op === "historyAfterRewrite") {
			const session = manager.SessionManager.openReadOnly(r.path);
			if (r.op === "historyAfterRemove") unlinkSync(r.path);
			else writeFileSync(r.path, r.contents);
			const entries: unknown[] = [];
			for await (const entry of history.readSessionHistoryReverse(session)) {
				entries.push(entry);
			}
			return entries;
		}
		if (r.op === "entriesEqual")
			return isDeepStrictEqual(
				manager.SessionManager.openReadOnly(r.left).getEntries(),
				manager.SessionManager.openReadOnly(r.right).getEntries(),
			);
		if (r.op === "migrateTools") {
			const tools = await import(
				pathToFileURL(join(root, "session-tool-output.ts")).href
			);
			return tools.migrateToolResultSessionFiles(
				r.dir,
				() => new Date(1792584000000),
			);
		}
		if (r.op === "extraTokens") {
			const entry = manager.SessionManager.openReadOnly(r.path)
				.getEntries()
				.find((e: any) => e.type === "compaction");
			return JSON.stringify([entry[String.fromCharCode(0xd800)], entry.usage]);
		}
		if (r.op === "exportReceiptTokens" || r.op === "exportExtraTokens") {
			const html = readFileSync(r.path, "utf8");
			const marker = '<script id="session-data" type="application/json">';
			const start = html.indexOf(marker) + marker.length;
			const end = html.indexOf("</script>", start);
			const data = JSON.parse(
				Buffer.from(html.slice(start, end), "base64").toString("utf8"),
			);
			if (r.op === "exportExtraTokens") {
				const entry = data.entries.find((e: any) => e.type === "compaction");
				return JSON.stringify([
					entry[String.fromCharCode(0xd800)],
					entry.usage,
				]);
			}
			return JSON.stringify(
				data.entries.find((e: any) => e.type === "message").mailboxStoreRefs,
			);
		}
		if (r.op === "receiptTokens") {
			const acceptance = await import(
				pathToFileURL(join(root, "runtime-mailbox-transcript.ts")).href
			);
			return JSON.stringify(
				acceptance.readAcceptedRuntimeMailboxStoreRefs(r.path),
			);
		}
		if (r.op === "scan") {
			const acceptance = await import(
				pathToFileURL(join(root, "runtime-mailbox-transcript.ts")).href
			);
			return acceptance.readAcceptedRuntimeMailboxStoreRefs(r.path);
		}
		if (r.op === "delete") {
			const deletion = await import(
				pathToFileURL(join(root, "session-delete.ts")).href
			);
			const previousPath = process.env.PATH;
			try {
				if (r.pathEnv !== undefined) process.env.PATH = r.pathEnv;
				const result = await deletion.deleteSessionTree(r.path, r.db);
				if (r.utf16) {
					if (result.ok) throw new Error("expected deletion failure");
					return {
						json: JSON.stringify(result),
						units: Array.from({ length: result.error.length }, (_, i) =>
							result.error.charCodeAt(i),
						),
					};
				}
				return result;
			} finally {
				process.env.PATH = previousPath;
			}
		}
		if (r.op === "archive")
			return archive.archivePersistedSession(r.db, r.path);
		if (r.op === "restore") return archive.restoreArchivedSession(r.db, r.path);
		if (r.op === "read") {
			const m = manager.SessionManager.openReadOnly(r.path);
			const entries = [];
			for await (const e of history.readSessionHistoryReverse(m))
				entries.push(e);
			return {
				id: m.getSessionId(),
				entries: m.getEntries(),
				history: entries,
				header: m.getHeader(),
				tree: m.getTree(),
				context: m.buildSessionContext().messages,
				text: transcript.readSessionTranscriptText(r.path),
			};
		}
		if (r.op === "transcript") {
			let streamed = "";
			if (r.stream !== false)
				for await (const chunk of transcript.createSessionTranscriptReadStream(
					r.path,
				))
					streamed += chunk;
			return {
				path: transcript.readSessionTranscriptPath(r.path),
				isTranscript: transcript.isSessionTranscriptFile(r.path),
				bytes: transcript.readSessionTranscriptBytes(r.path).toString("base64"),
				text: transcript.readSessionTranscriptText(r.path),
				streamed,
			};
		}
		if (r.op === "predicates")
			return r.paths.map((path: string) => ({
				transcript: transcript.isSessionTranscriptFile(path),
				archive: archive.isArchivedSessionFile(path),
				resolved: transcript.readSessionTranscriptPath(path),
			}));
		if (r.op === "fileArchive") return archive.archiveSessionFile(r.path);
		if (r.op === "fileRestore")
			return archive.restoreArchivedSessionFile(r.path);
		if (r.op === "info") {
			const s = await manager.buildSessionInfo(r.path);
			return s
				? {
						path: s.path,
						id: s.id,
						cwd: s.cwd,
						messageCount: s.messageCount,
						firstMessage: s.firstMessage,
					}
				: null;
		}
		if (r.op === "list")
			return (
				await manager.SessionManager.list(r.cwd, r.dir, undefined, r.db)
			).map((s: any) => ({
				path: s.path,
				id: s.id,
				cwd: s.cwd,
				messageCount: s.messageCount,
				firstMessage: s.firstMessage,
			}));
		if (r.op === "metadata") {
			const db = await import(
				pathToFileURL(join(root, "session-control-db.ts")).href
			);
			return db.readSessionMetadata(r.db, r.path) ?? null;
		}
		if (r.op === "mutable")
			return manager.SessionManager.open(r.path).getSessionId();
		if (r.op === "context") return manager.loadSessionContextEntries(r.path);
		if (r.op === "load") return manager.loadEntriesFromFile(r.path);
		if (r.op === "header") return manager.readSessionHeader(r.path);
		if (r.op === "bytes")
			return {
				bytes: readFileSync(r.path).toString("base64"),
				mode: statSync(r.path).mode & 0o777,
			};
		throw new Error(`Unknown operation: ${r.op}`);
	}
	for await (const line of createInterface({ input: process.stdin })) {
		const r = JSON.parse(line);
		try {
			console.log(
				JSON.stringify({ id: r.id, ok: true, output: await handle(r) }),
			);
		} catch (e) {
			const details =
				r.op === "historyAfterRewrite" && e instanceof Error
					? {
							name: e.name,
							code: "code" in e ? e.code : undefined,
							errno: "errno" in e ? e.errno : undefined,
						}
					: undefined;
			console.log(
				JSON.stringify({ id: r.id, ok: false, error: String(e), details }),
			);
		}
	}
}
void main();
