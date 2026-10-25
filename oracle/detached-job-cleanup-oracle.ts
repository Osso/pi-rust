import { existsSync } from "node:fs";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type CleanupInput = {
	database: string;
	artifactRoot: string;
	now: number;
	paths: string[];
	mode: "direct" | "wrapper" | "references";
};
type CleanupApi = {
	readLinuxProcessReferences(): Set<string> | undefined;
	cleanupDetachedJobArtifacts(
		database: string,
		options: { artifactRoot: string; now: number },
	): unknown;
	runDetachedJobArtifactCleanup(
		database: string,
		artifactRoot: string,
		now: number,
	): void;
};

void runOracle((module, request) => {
	const api = module as unknown as CleanupApi;
	const input = request as unknown as CleanupInput;
	if (input.mode === "references") {
		const references = api.readLinuxProcessReferences();
		if (!references) throw new Error("Linux process references unavailable");
		return input.paths.map((path) => references.has(path));
	}
	if (input.mode === "wrapper") {
		api.runDetachedJobArtifactCleanup(
			input.database,
			input.artifactRoot,
			input.now,
		);
		return { exists: input.paths.map((path) => existsSync(path)) };
	}
	return api.cleanupDetachedJobArtifacts(input.database, {
		artifactRoot: input.artifactRoot,
		now: input.now,
	});
}).catch((error: unknown) => {
	process.stderr.write(
		`detached cleanup oracle startup failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
