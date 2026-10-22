import { join } from "node:path";
import { pathToFileURL } from "node:url";

type AssistantStream = { push(event: unknown): void };

export function normalizeAssistantMessageClock(message: unknown): unknown {
	const copy = structuredClone(message);
	if (copy !== null && typeof copy === "object" && "timestamp" in copy) {
		(copy as Record<string, unknown>).timestamp = 0;
	}
	return copy;
}

export function normalizeAssistantEventClocks(event: unknown): unknown {
	const copy = structuredClone(event);
	if (copy !== null && typeof copy === "object" && !Array.isArray(copy)) {
		const record = copy as Record<string, unknown>;
		for (const key of ["partial", "message", "error"]) {
			if (record[key] !== undefined)
				record[key] = normalizeAssistantMessageClock(record[key]);
		}
	}
	return copy;
}

/** Apply the approved owned-event contract at push, before queue aliasing. */
export async function installAssistantSnapshots(
	sourceDirectory: string,
): Promise<void> {
	const source = (await import(
		pathToFileURL(join(sourceDirectory, "utils/event-stream.ts")).href
	)) as {
		AssistantMessageEventStream: { prototype: AssistantStream };
	};
	const prototype = source.AssistantMessageEventStream.prototype;
	const push = prototype.push;
	prototype.push = function (this: AssistantStream, event: unknown): void {
		push.call(this, structuredClone(event));
	};
}
