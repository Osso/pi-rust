// Oracle for `core/trust-manager.ts`: runs a step list against a real ProjectTrustStore and returns
// the results plus the resulting trust.json bytes.
import { serve } from "./config-auth-oracle-support.ts";

type Decision = boolean | null;

type Step =
	| { kind: "get"; cwd: string }
	| { kind: "set"; cwd: string; decision: Decision }
	| { kind: "setMany"; updates: Array<{ path: string; decision: Decision }> }
	| { kind: "options"; cwd: string; includeSessionOnly?: boolean }
	| { kind: "parent"; cwd: string }
	| { kind: "resources"; cwd: string };

type TrustApi = {
	ProjectTrustStore: new (agentDir: string) => {
		get(cwd: string): Decision;
		getEntry(cwd: string): { path: string; decision: boolean } | null;
		set(cwd: string, decision: Decision): void;
		setMany(decisions: Array<{ path: string; decision: Decision }>): void;
	};
	getProjectTrustParentPath(cwd: string): string | undefined;
	getProjectTrustOptions(cwd: string, options?: { includeSessionOnly?: boolean }): unknown[];
	hasTrustRequiringProjectResources(cwd: string): boolean;
};

type Request = {
	id: unknown;
	agentDir: string;
	steps: Step[];
	env?: Record<string, string | null>;
};

function withEnvironment<T>(values: Record<string, string | null> | undefined, action: () => T): T {
	if (!values) return action();
	const previous = new Map<string, string | undefined>();
	for (const [name, value] of Object.entries(values)) {
		previous.set(name, process.env[name]);
		if (value === null) delete process.env[name];
		else process.env[name] = value;
	}
	try {
		return action();
	} finally {
		for (const [name, value] of previous) {
			if (value === undefined) delete process.env[name];
			else process.env[name] = value;
		}
	}
}

async function main(): Promise<void> {
	const { readFileSync, existsSync } = await import("node:fs");
	const { join } = await import("node:path");
	const { pathToFileURL } = await import("node:url");
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("trust oracle requires a source module path");
	const api = (await import(pathToFileURL(sourcePath).href)) as TrustApi;

	serve((raw) => {
		const request = raw as Request;
		const store = new api.ProjectTrustStore(request.agentDir);
		const results: unknown[] = [];
		return withEnvironment(request.env, () => {
			try {
				for (const step of request.steps) {
					results.push(runStep(step, store, api));
				}
			} catch (error) {
				return {
					results,
					error: { name: (error as Error).name, message: (error as Error).message },
					trustFile: readTrustFile(join(request.agentDir, "trust.json")),
				};
			}
			return { results, trustFile: readTrustFile(join(request.agentDir, "trust.json")) };
		});

		function readTrustFile(path: string): string | null {
			return existsSync(path) ? readFileSync(path, "utf-8") : null;
		}
	});
}

function runStep(
	step: Step,
	store: InstanceType<TrustApi["ProjectTrustStore"]>,
	api: TrustApi,
): unknown {
	switch (step.kind) {
		case "get":
			return store.get(step.cwd);
		case "set":
			store.set(step.cwd, step.decision);
			return null;
		case "setMany":
			store.setMany(step.updates);
			return null;
		case "options":
			return api.getProjectTrustOptions(step.cwd, { includeSessionOnly: step.includeSessionOnly });
		case "parent":
			return api.getProjectTrustParentPath(step.cwd) ?? null;
		case "resources":
			return api.hasTrustRequiringProjectResources(step.cwd);
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`trust oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
