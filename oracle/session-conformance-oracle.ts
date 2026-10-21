import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import type { createSessionBackendConformance } from "@pi-oracle-source/agent/src/harness/session/testing/index.ts";
import type { InMemorySessionRepo } from "@pi-oracle-source/agent/src/harness/session/memory.ts";

type Api = {
	createSessionBackendConformance: typeof createSessionBackendConformance;
};
type MemoryApi = { InMemorySessionRepo: typeof InMemorySessionRepo };
type Request = { op: "manifest" } | { op: "run"; index: number };

async function main(): Promise<void> {
	const path = process.argv[2];
	if (path === undefined)
		throw new Error("session conformance oracle requires source module path");
	const url = pathToFileURL(path);
	const api = (await import(url.href)) as Api;
	const memory = (await import(new URL("../memory.ts", url).href)) as MemoryApi;
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line) as Request;
		let created = 0;
		let disposed = 0;
		const cases = api.createSessionBackendConformance(async () => {
			created++;
			return {
				repository: new memory.InMemorySessionRepo(),
				async [Symbol.asyncDispose]() {
					disposed++;
				},
			};
		});
		if (request.op === "manifest") {
			process.stdout.write(
				`${JSON.stringify({ cases: cases.map(({ group, name }) => ({ group, name })), created, disposed })}\n`,
			);
			continue;
		}
		const test = cases[request.index];
		if (test === undefined)
			throw new Error(`missing conformance case ${request.index}`);
		await test.run();
		process.stdout.write(
			`${JSON.stringify({ group: test.group, name: test.name, created, disposed, success: true })}\n`,
		);
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
