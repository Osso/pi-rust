// Oracle for `core/permissions/rule-store.ts`: writes rules into real settings files and reports the
// resulting bytes, plus `buildPermissionRuleContent` and in-memory store behavior.
import { serve } from "./config-auth-oracle-support.ts";

type Destination = "session" | "userSettings" | "projectSettings" | "localSettings";

type Request =
	| {
			id: unknown;
			op: "write";
			cwd: string;
			agentDir: string;
			destination: Exclude<Destination, "session">;
			toolName: string;
			rules: string[];
			behavior: "allow";
	  }
	| { id: unknown; op: "content"; toolName: string; input: Record<string, unknown> }
	| {
			id: unknown;
			op: "store";
			steps: Array<
				| { kind: "add"; toolName: string; rules: string[] }
				| { kind: "has"; toolName: string; rule: string }
				| { kind: "settings"; settings: unknown }
				| {
						kind: "apply";
						toolName: string;
						updates: Array<{ destination: Destination; behavior: "allow"; rules: string[] }>;
						cwd?: string;
						agentDir?: string;
				  }
			>;
	  };

type RuleStoreApi = {
	PermissionRuleStore: new (options?: {
		cwd?: string;
		agentDir?: string;
		writer?: (options: unknown) => void;
		settings?: { permissionRules?: unknown };
	}) => {
		hasAllowRule(toolName: string, ruleContent: string): boolean;
		addAllowRules(toolName: string, rules: string[]): void;
		addSettingsRules(settings: { permissionRules?: unknown } | undefined): void;
		applyUpdatedPermissions(toolName: string, updates: unknown[] | undefined): void;
	};
	buildPermissionRuleContent(toolName: string, input: Record<string, unknown>): string;
	writePermissionRules(options: unknown): void;
};

async function main(): Promise<void> {
	const { readFileSync, existsSync } = await import("node:fs");
	const { join } = await import("node:path");
	const { pathToFileURL } = await import("node:url");
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("rule store oracle requires a source module path");
	const api = (await import(pathToFileURL(sourcePath).href)) as RuleStoreApi;

	serve((raw) => {
		const request = raw as Request;
		if (request.op === "content") {
			return { content: api.buildPermissionRuleContent(request.toolName, request.input) };
		}
		if (request.op === "write") {
			const writes: unknown[] = [];
			api.writePermissionRules({
				agentDir: request.agentDir,
				behavior: request.behavior,
				cwd: request.cwd,
				destination: request.destination,
				rules: request.rules,
				toolName: request.toolName,
			});
			const path = settingsPath(request);
			return {
				content: existsSync(path) ? readFileSync(path, "utf-8") : null,
				writes,
			};
		}
		const writes: unknown[] = [];
		const store = new api.PermissionRuleStore({
			cwd: undefined,
			agentDir: undefined,
			writer: (options) => writes.push(options),
		});
		const results: unknown[] = [];
		for (const step of request.steps) {
			if (step.kind === "add") {
				store.addAllowRules(step.toolName, step.rules);
				results.push(null);
			} else if (step.kind === "has") {
				results.push(store.hasAllowRule(step.toolName, step.rule));
			} else if (step.kind === "settings") {
				store.addSettingsRules(step.settings as { permissionRules?: unknown });
				results.push(null);
			} else {
				const configured =
					step.cwd !== undefined || step.agentDir !== undefined
						? new api.PermissionRuleStore({ cwd: step.cwd, agentDir: step.agentDir, writer: (options) => writes.push(options) })
						: store;
				configured.applyUpdatedPermissions(step.toolName, step.updates);
				results.push(null);
			}
		}
		return { results, writes };
	});

	function settingsPath(request: Extract<Request, { op: "write" }>): string {
		if (request.destination === "userSettings") return join(request.agentDir, "settings.json");
		if (request.destination === "projectSettings") return join(request.cwd, ".pi", "settings.json");
		return join(request.cwd, ".pi", "settings.local.json");
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`rule store oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
