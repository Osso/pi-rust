// Differential oracle for the bundled `default-footer` extension. Renders the
// real TypeScript component with fake context/footer data for concrete inputs.

import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type AnyRecord = Record<string, any>;

const codingAgentDir = dirname(dirname(dirname(dirname(process.argv[2] ?? ""))));

async function loadSource(relative: string): Promise<AnyRecord> {
	return (await import(pathToFileURL(join(codingAgentDir, relative)).href)) as AnyRecord;
}

function stripAnsi(value: string): string {
	return value.replace(/\u001b\[[0-9;]*m/g, "");
}

function usageEntries(input: AnyRecord): AnyRecord[] {
	return [
		{
			type: "message",
			message: {
				role: "assistant",
				usage: {
					input: input.usage?.input ?? 0,
					output: input.usage?.output ?? 0,
					cacheRead: input.usage?.cacheRead ?? 0,
					cacheWrite: input.usage?.cacheWrite ?? 0,
					cost: { total: input.usage?.cost ?? 0 },
				},
			},
		},
	];
}

type LifecycleContext = {
	ui: {
		setStatus(key: string, text?: string): void;
		requestRender(): void;
		setDefaultFooter(factory: unknown): void;
	};
};
type LifecycleHandler = (event: Record<string, never>, ctx: LifecycleContext) => Promise<void>;
type LifecycleRegistration = (pi: { on(name: string, handler: LifecycleHandler): void }) => void;

async function runLifecycle(register: LifecycleRegistration): Promise<unknown[]> {
	const handlers = new Map<string, LifecycleHandler>();
	const calls: unknown[] = [];
	register({ on: (name, handler) => { handlers.set(name, handler); } });
	const ctx: LifecycleContext = { ui: {
		setStatus: (key, text) => { calls.push(["status", key, text ?? null]); },
		requestRender: () => { calls.push(["render"]); },
		setDefaultFooter: () => { calls.push(["footer"]); },
	} };
	for (const name of ["background_compaction_start", "background_compaction_end", "background_compaction_start", "session_shutdown", "session_start"]) {
		const handler = handlers.get(name);
		if (!handler) throw new Error(`Missing footer handler: ${name}`);
		await handler({}, ctx);
	}
	return calls;
}

void runOracle(async (api, request) => {
	if (request.operation === "lifecycle") return runLifecycle(api.default as LifecycleRegistration);
	const createFooter = api.createDefaultFooterComponent as (
		input: Record<string, unknown>,
	) => { render(width: number): string[] };
	const cases = requireCases(request) as AnyRecord[];
	const themeModule = await loadSource("src/modes/interactive/theme/theme.ts");
	themeModule.initTheme(undefined, false);
	const theme = themeModule.theme;
	const results: AnyRecord[] = [];
	for (const input of cases) {
		const override = input.override;
		const sessionManager = override
			? {
					getCwd: () => override.cwd,
					getEntries: () => usageEntries(override),
					getSessionName: () => override.sessionName ?? "",
				}
			: {
					getCwd: () => input.cwd ?? "/tmp/project",
					getEntries: () => usageEntries(input),
					getSessionName: () => input.sessionName ?? "",
				};
		const model = override?.model ?? input.model ?? { id: "test-model", provider: "test", contextWindow: 272_000, reasoning: false };
		const overrideValue = override ? { ...override, sessionManager } : undefined;
		const ctx = {
			model,
			sessionManager,
			getContextUsage: () => (override ? override.contextUsage : input.contextUsage),
			getThinkingLevel: () => override?.thinkingLevel ?? input.thinkingLevel ?? "off",
		};
		const footerData = {
			getAvailableProviderCount: () => input.availableProviderCount ?? 1,
			getExecutableName: () => input.executableName,
			getExtensionStatuses: () => new Map(Object.entries(input.extensionStatuses ?? {})),
			getGitBranch: () => (override ? null : (input.gitBranch ?? null)),
			getSessionOverride: () => overrideValue,
			onBranchChange: () => () => {},
		};
		const component = createFooter({
			ctx,
			footerData,
			getAgentCounts: input.agentCounts ? () => input.agentCounts : undefined,
			getSelectedAgent: input.selectedAgent ? () => input.selectedAgent : undefined,
			theme,
		});
		results.push({
			name: input.name,
			lines: component.render(input.width ?? 160).map(stripAnsi),
		});
	}
	return results;
});
