// Real c248b0067 session-autoname extension; all generated names/errors are captured.
import { dirname, resolve } from "node:path";
import { createInterface } from "node:readline";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const extensionPath = process.argv[2];
	const packages = resolve(dirname(extensionPath), "../../../..");
	const { default: register } = await import(pathToFileURL(extensionPath).href);
	const ai = await import(pathToFileURL(resolve(packages, "ai/src/compat.ts")).href);

	for await (const line of createInterface({ input: process.stdin })) {
		if (!line.trim()) continue;
		const request = JSON.parse(line);
		const faux = ai.registerFauxProvider({ api: "faux", tokenSize: { min: 1, max: 1 }, tokensPerSecond: 1000000 });
		const names: string[] = [];
		const errors: string[] = [];
		const handlers = new Map<string, Function>();
		let stored = false;
		const originalError = console.error;
		console.error = (...values: unknown[]) => errors.push(values.map(String).join(" "));
		try {
			faux.setResponses([ai.fauxAssistantMessage(request.title)]);
			const manager = {
				getSessionFile: () => "/fixture/session.jsonl",
				isSubagentSession: () => false,
				hasSessionNameState: () => stored,
				getBranch: () => [{ type: "message", message: { role: "user", content: "Fix schema migration", timestamp: 1 } }],
			};
			const pi = {
				on: (type: string, handler: Function) => handlers.set(type, handler),
				setSessionName: (name: string) => { names.push(name); stored = true; handlers.get("session_info_changed")?.({ name }); },
			};
			const ctx = {
				mode: "tui", sessionManager: manager, model: faux.getModel(),
				modelRegistry: { getApiKeyAndHeaders: async () => ({ ok: true, apiKey: "faux-key" }) },
			};
			register(pi);
			handlers.get("agent_end")?.({ messages: [{ role: "user", content: "Fix schema migration", timestamp: 1 }] }, ctx);
			const deadline = Date.now() + 2000;
			while (names.length === 0 && errors.length === 0 && Date.now() < deadline) {
				await new Promise((done) => setTimeout(done, 2));
			}
            if (request.probeUTF16) {
                const db = await import(pathToFileURL(resolve(packages, "coding-agent/src/core/session-control-db.ts")).href);
                const root = mkdtempSync(resolve(tmpdir(), "pi-autoname-utf16-"));
                try {
                    const path = resolve(root, "control.sqlite");
                    db.writeSessionMetadata(path, {
                        sessionPath: "/fixture/session.jsonl", id: "fixture", cwd: root,
                        createdAt: "2026-10-20T00:00:00.000Z", modifiedAt: "2026-10-20T00:00:00.000Z",
                        messageCount: 0, firstMessage: "(no messages)", allMessagesText: "",
                    });
                    db.writeSessionName(path, "/fixture/session.jsonl", names[0]);
                    process.stdout.write(`${JSON.stringify({
                        helperNameJson: JSON.stringify(names[0]),
                        helperUnits: Array.from({ length: names[0].length }, (_, index) => names[0].charCodeAt(index)),
                        storedNameJson: JSON.stringify(db.readSessionMetadata(path, "/fixture/session.jsonl").name),
                    })}\n`);
                } finally { rmSync(root, { recursive: true, force: true }); }
            } else {
                process.stdout.write(`${JSON.stringify({ names, errors, calls: faux.state.callCount, hasStoredValue: stored })}\n`);
            }
		} finally {
			console.error = originalError;
			faux.unregister();
		}
	}
}

void main();
