import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";

export async function runEffortPrefixScenario(context) {
	const {
		session,
		sessionManager,
		settingsManager,
		modelRegistry,
		modules,
		dir,
		agent,
	} = context;
	const root = resolve(dirname(process.argv[2]), "../../..");
	const { Type } = createRequire(process.argv[2])("typebox");
	const { loadExtensionFromFactory } = await import(
		`${root}/coding-agent/src/core/extensions/loader.ts`
	);
	const { createEventBus } = await import(
		`${root}/coding-agent/src/core/event-bus.ts`
	);
	const { getThemeByName } = await import(
		`${root}/coding-agent/src/modes/interactive/theme/theme.ts`
	);
	const { default: effort } = await import(
		`${root}/coding-agent/extensions/effort/src/index.ts`
	);
	const statuses = new Map();
	const runtime = modules.createExtensionRuntime();
	const observe = () => ({
		mode: sessionManager
			.getEntries()
			.slice()
			.reverse()
			.find(
				(entry) =>
					entry.type === "custom" && entry.customType === "multi-agent-mode",
			)?.data?.mode,
		tools: session.getActiveToolNames(),
		status: statuses
			.get("multi-agent-mode")
			?.replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, ""),
	});
	try {
		const extension = await loadExtensionFromFactory(
			(api) => {
				effort(api);
				api.registerTool({
					name: "spawn_agent",
					label: "Spawn",
					description: "Unused fixture tool",
					parameters: Type.Object({}),
					execute: async () => ({ content: [] }),
				});
			},
			dir,
			createEventBus(),
			runtime,
		);
		const runner = new modules.ExtensionRunner(
			[extension],
			runtime,
			dir,
			sessionManager,
			modelRegistry,
			settingsManager,
		);
		session._extensionRunner = runner;
		session._bindExtensionCore(runner);
		runner.setUIContext(
			{
				...runner.getUIContext(),
				theme: getThemeByName("dark"),
				setStatus: (key, value) => {
					if (value === undefined) statuses.delete(key);
					else statuses.set(key, value);
				},
			},
			"tui",
		);
		session._refreshToolRegistry({ activeToolNames: ["spawn_agent"] });
		session.setActiveToolsByName(["spawn_agent"]);
		await session.prompt("/multi-agent disabled");
		const before = observe();
		agent.state.model = {
			...agent.state.model,
			thinkingLevelMap: { ultra: "high" },
		};
		session.setThinkingLevel("ultra");
		const after = observe();
		return { before, after };
	} finally {
		session.dispose();
	}
}
