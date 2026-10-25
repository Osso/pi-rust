import { dirname, resolve } from "node:path";

async function bindThinkingListener(context, handler) {
	const {
		session,
		sessionManager,
		settingsManager,
		modelRegistry,
		modules,
		dir,
	} = context;
	const root = resolve(dirname(process.argv[2]), "../../..");
	const { loadExtensionFromFactory } = await import(
		`${root}/coding-agent/src/core/extensions/loader.ts`
	);
	const { createEventBus } = await import(
		`${root}/coding-agent/src/core/event-bus.ts`
	);
	const runtime = modules.createExtensionRuntime();
	const extension = await loadExtensionFromFactory(
		(api) => api.on("thinking_level_select", handler),
		dir,
		createEventBus(),
		runtime,
	);
	session._extensionRunner = new modules.ExtensionRunner(
		[extension],
		runtime,
		dir,
		sessionManager,
		modelRegistry,
		settingsManager,
	);
}

export async function runThinkingListenerScenario(context) {
	const views = [];
	try {
		await bindThinkingListener(context, (event) => {
			views.push({ level: event.level, previousLevel: event.previousLevel });
		});
		context.session.setThinkingLevel("high");
		return views.slice();
	} finally {
		context.session.dispose();
	}
}

export async function runThinkingPendingScenario(context) {
	const views = [];
	let release;
	let settled;
	const gate = new Promise((resolve) => {
		release = resolve;
	});
	const done = new Promise((resolve) => {
		settled = resolve;
	});
	try {
		await bindThinkingListener(context, async () => {
			views.push("start");
			await gate;
			views.push("finish");
			settled();
		});
		context.session.setThinkingLevel("high");
		const immediate = views.slice();
		release();
		await done;
		return { immediate, settled: views.slice() };
	} finally {
		release();
		context.session.dispose();
	}
}
