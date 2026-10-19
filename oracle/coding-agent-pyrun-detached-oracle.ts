import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type WriteCase = {
	dir: string;
	data: Record<string, unknown>;
	identity: Record<string, unknown>;
};

type ReadCase = {
	content: string;
	dir: string;
};

async function main(): Promise<void> {
	const api = (await import(process.argv[2] ?? "")) as unknown as {
		readDetachedPyrunActivation(path: string): unknown;
		readDetachedPyrunLaunchManifest(path: string): unknown;
		writeDetachedPyrunActivation(path: string, identity: unknown): void;
		writeDetachedPyrunLaunchManifest(path: string, data: unknown): void;
	};
	await runOracle((_module, request) => {
		const cases = requireCases(request);
		if (request.operation === "write") {
			return (cases as WriteCase[]).map((input) => {
				mkdirSync(input.dir, { recursive: true });
				const manifestPath = `${input.dir}/launch.json`;
				api.writeDetachedPyrunLaunchManifest(manifestPath, input.data);
				const content = readFileSync(manifestPath, "utf8");
				const activationPath = `${input.dir}/activation.json`;
				api.writeDetachedPyrunActivation(activationPath, input.identity);
				const activation = readFileSync(activationPath, "utf8");
				return {
					activation,
					content,
					readActivation: JSON.stringify(api.readDetachedPyrunActivation(activationPath)),
					readManifest: JSON.stringify(api.readDetachedPyrunLaunchManifest(manifestPath)),
				};
			});
		}
		if (request.operation === "readInvalid") {
			return (cases as ReadCase[]).map((input) => {
				mkdirSync(input.dir, { recursive: true });
				const manifestPath = `${input.dir}/launch.json`;
				writeFileSync(manifestPath, input.content);
				try {
					return { value: JSON.stringify(api.readDetachedPyrunLaunchManifest(manifestPath)) };
				} catch (error) {
					return { error: error instanceof Error ? error.message : String(error) };
				}
			});
		}
		throw new Error(`unsupported operation ${request.operation}`);
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`coding-agent pyrun detached oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
