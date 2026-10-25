import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	const { createSessionSetupWriteView } = await import(pathToFileURL(modulePath).href);
	const { SessionManager } = await import(pathToFileURL(join(dirname(modulePath), "../session-manager.ts")).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const session = SessionManager.inMemory("/private/context");
		const view = createSessionSetupWriteView(session);
		const data = { nested: { value: 1 } };
		const message = { role: "user", content: [{ type: "text", text: "original" }], timestamp: 1 };
		const ids = [view.appendMessage(message), view.appendCustomEntry("setup", data), view.appendCustomMessageEntry("notice", "text", true, data)];
		ids.push(view.appendCompaction("summary", ids[0], 123, data));
		ids.push(view.branchWithSummary(ids[0], "branch", data));
		data.nested.value = 99;
		message.content[0].text = "mutated";
		const entries = ids.map((id) => session.getEntry(id));
		const output = [entries[0].message, entries[1].data, entries[2].details, entries[3].details, entries[4].details];
		process.stdout.write(`${JSON.stringify({ id: request.id, output, cwd: view.getCwd() })}\n`);
	}
}
void main().catch((error: unknown) => { console.error(error); process.exitCode = 1; });
