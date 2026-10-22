import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { createContext, runInContext } from "node:vm";

type Step = { timestamp: number; random: number[] };
type UuidApi = { uuidv7(): string };
type Request = { id: number; operation: "generate" | "vectors"; count?: number; steps?: Step[] };

function generateVectors(code: string, steps: Step[]): string[] {
	let index = 0;
	const module = { exports: {} };
	const context = createContext({
		module,
		exports: module.exports,
		Date: { now: () => steps[index]!.timestamp },
		crypto: { getRandomValues(bytes: Uint8Array) { bytes.set(steps[index]!.random); return bytes; } },
	});
	runInContext(code, context);
	const api = module.exports as UuidApi;
	return steps.map(() => {
		const value = api.uuidv7();
		index++;
		return value;
	});
}

async function main(): Promise<void> {
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("requires the frozen uuid.ts file");
	const sourceUrl = pathToFileURL(sourcePath);
	const api = await import(sourceUrl.href) as UuidApi;
	const require = createRequire(sourceUrl);
	const esbuild = require("esbuild") as { transformSync(code: string, options: object): { code: string } };
	const code = esbuild.transformSync(readFileSync(sourcePath, "utf8"), { loader: "ts", format: "cjs", target: "es2022" }).code;
	for await (const line of createInterface({ input: process.stdin })) {
		let id: number | null = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			let output: unknown;
			if (request.operation === "vectors") {
				output = generateVectors(code, request.steps ?? []);
			} else {
				const before = Date.now();
				const values = Array.from({ length: request.count ?? 0 }, () => api.uuidv7());
				output = { before, after: Date.now(), values };
			}
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			const normalized = error instanceof Error ? { name: error.name, message: error.message } : { name: "UnknownError", message: String(error) };
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalized })}\n`);
		}
	}
}
void main().catch((error: unknown) => { console.error(error); process.exitCode = 1; });
