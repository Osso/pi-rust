import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const { appendSelfRestartNotice } = await import(
	pathToFileURL(`${source}/core/self-restart.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${source}/core/session-manager.ts`).href
);

interface Request {
	root: string;
	prompt?: string;
	oldPid: "same" | number;
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		const manager = SessionManager.create(
			request.root,
			join(request.root, "sessions"),
		);
		appendSelfRestartNotice(manager, {
			sessionFile: manager.getSessionFile(),
			prompt: request.prompt,
			oldPid: request.oldPid === "same" ? process.pid : request.oldPid,
		});
		manager.appendMessage({ role: "assistant", content: [], timestamp: 0 });
		const reopened = SessionManager.open(manager.getSessionFile());
		const notice = reopened
			.getEntries()
			.find((entry: { type: string }) => entry.type === "custom_message");
		console.log(
			JSON.stringify({
				pid: process.pid,
				notice: notice
					? {
							customType: notice.customType,
							content: notice.content,
							display: notice.display,
						}
					: null,
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ error: String(error) }));
	}
}
