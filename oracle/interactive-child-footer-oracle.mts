import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const receiver = {
			childViewAgentId: "child",
			childViewSessionManager: {
				buildSessionContext: () => ({ model: request.transcript, thinkingLevel: "off", messages: [] }),
				readPersistedSessionSettings: () => request.persisted,
				getCwd: () => request.cwd,
			},
			multiAgentStore: { getAgent: () => ({ model: request.published }) },
			session: { modelRegistry: { find: (provider: string, id: string) => request.models.find((model) => model.provider === provider && model.id === id) } },
		};
		const result = InteractiveMode.prototype.buildSelectedAgentFooterOverride.call(receiver);
		console.log(JSON.stringify({ ok: true, output: { model: result.model?.id ?? null, thinking: result.thinkingLevel, contextWindow: result.contextUsage?.contextWindow ?? null } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
