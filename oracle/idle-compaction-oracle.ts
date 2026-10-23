import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	serve((request) => ({
		prototypeNames: Object.getOwnPropertyNames(Object.prototype),
		results: request.messages.map((message: unknown) => {
			const result = api.computeIdleCompactionDueAt(message);
			return result === undefined ? { kind: "undefined" } : Number.isNaN(result) ? { kind: "nan" } : { kind: "number", value: result };
		}),
	}));
}
void main();
