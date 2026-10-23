import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	serve(() => {
		const manager = api.SessionManager.inMemory("/tmp");
		const message = { role: "user", content: "same", timestamp: 1 };
		const first = manager.appendMessage(message);
		const second = manager.appendMessage({ ...message });
		manager.appendCustomMessageEntry("notice", "synthetic", true);
		const left = manager.buildSessionContext().messages;
		const right = manager.buildSessionContext().messages;
		return {
			storedSame: left.slice(0, 2).map((entry: object, index: number) => entry === right[index]),
			equalValuesDistinct: left[0] !== left[1],
			syntheticSame: left[2] === right[2],
			firstIsInput: manager.getEntry(first).message === message,
			secondIsInput: manager.getEntry(second).message === message,
		};
	});
}
void main();
