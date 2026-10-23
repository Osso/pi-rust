import { pathToFileURL } from "node:url";
import { importSource, serve } from "./config-auth-oracle-support.ts";

const user = (text: string) => ({ role: "user", content: text, timestamp: 0 });
async function main(): Promise<void> {
	const api = await importSource<any>();
	const { AssistantMessageEventStream } = await import(new URL("../../ai/src/utils/event-stream.ts", pathToFileURL(process.argv[2])).href);
	serve(async (request) => {
		if (request.op === "dispatch") {
			const agent = new api.Agent({ streamFn: () => {
				const stream = new AssistantMessageEventStream();
				const message = { role: "assistant", content: [], api: "unknown", provider: "unknown", model: "unknown", stopReason: "stop", timestamp: 0, usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } } };
				queueMicrotask(() => stream.push({ type: "done", reason: "stop", message }));
				return stream;
			} });
			const observations: unknown[] = [];
			agent.subscribe((event: any) => {
				if (event.type === "message_end" && event.message.role === "user") agent.state.messages.push(user("inserted by earlier listener"));
			});
			agent.subscribe((event: any) => {
				if (event.type === "message_end" && event.message.role === "user") {
					observations.push({ eventStored: agent.state.messages.includes(event.message), eventIsLast: agent.state.messages.at(-1) === event.message, count: agent.state.messages.length });
				}
			});
			await agent.prompt("prompt");
			return observations;
		}
		const agent = new api.Agent({ initialState: { messages: [user("original")] } });
		const model = agent.state.model;
		const original = [...agent.state.messages];
		const result = [];
		for (const operation of request.operations as string[]) {
			if (operation === "append") agent.state.messages.push(user("extra"));
			else if (operation === "appendStatus") agent.state.messages.push({ role: "custom", customType: "supervisor-status", content: "status", display: true, timestamp: 0 });
			else if (operation === "truncate") agent.state.messages = agent.state.messages.slice(0, 1);
			else if (operation === "copyArray") agent.state.messages = agent.state.messages.slice();
			else if (operation === "replaceMessages") agent.state.messages = agent.state.messages.map((message: object) => ({ ...message }));
			else if (operation === "replaceModel") agent.state.model = { ...agent.state.model };
			else if (operation === "systemPrompt") agent.state.systemPrompt = "changed";
			else if (operation === "tools") agent.state.tools = [];
			else if (operation === "reset") agent.reset();
			else if (operation === "appendOriginalCopy") agent.state.messages.push({ ...original[0] });
			const messages = agent.state.messages.filter((message: any) => message.role !== "custom" || message.customType !== "supervisor-status");
			result.push({ modelSame: agent.state.model === model, count: messages.length, prefixSame: original.every((message, index) => messages[index] === message), matches: agent.state.model === model && messages.length === original.length && original.every((message, index) => messages[index] === message) });
		}
		return result;
	});
}
void main();
