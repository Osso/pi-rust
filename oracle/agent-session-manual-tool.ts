import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import {
	fauxAssistantMessage,
	fauxToolCall,
} from "@earendil-works/pi-ai/compat";

export async function runManualToolCompaction(context) {
	const {
		session,
		sessionManager,
		settingsManager,
		modelRegistry,
		modules,
		dir,
		core,
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
	let signal;
	let started;
	let release;
	const pending = new Promise((resolve) => {
		started = resolve;
	});
	const gate = new Promise((resolve) => {
		release = resolve;
	});
	const runtime = modules.createExtensionRuntime();
	try {
		const savedRetry = [];
		const extension = await loadExtensionFromFactory(
			(api) => {
				api.registerTool({
					name: "hold",
					label: "Hold",
					description: "Hold until explicitly released",
					parameters: Type.Object({}),
					execute: async (_id, _params, abort) => {
						signal = abort;
						started();
						await gate;
						return { content: [] };
					},
				});
				api.on("session_compact", (event) => {
					savedRetry.push(event.willRetry);
				});
				api.on("compaction", (event) => ({
					compaction: {
						summary: "saved summary",
						firstKeptEntryId: event.preparation.firstKeptEntryId,
						tokensBefore: event.preparation.tokensBefore,
					},
				}));
			},
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
		session._bindExtensionCore(session._extensionRunner);
		session._refreshToolRegistry({ activeToolNames: ["hold"] });
		session.setActiveToolsByName(["hold"]);
		settingsManager.setCompactionEnabled(false);
		settingsManager.setRetryEnabled(false);
		for (let i = 0; i < 4; i++) {
			sessionManager.appendMessage({
				role: "user",
				content: `history ${i}`,
				timestamp: i * 2,
			});
			sessionManager.appendMessage(fauxAssistantMessage(`history answer ${i}`));
		}
		agent.state.messages = sessionManager.buildSessionContext().messages;
		const retry = [];
		session.subscribe((event) => {
			if (event.type === "compaction_end") retry.push(event.willRetry);
		});
		core.setResponses([
			fauxAssistantMessage([fauxToolCall("hold", {}, { id: "hold-call" })], {
				stopReason: "toolUse",
			}),
			fauxAssistantMessage("resumed"),
		]);
		const prompt = session.prompt("start tool");
		const toolStarted = await Promise.race([
			pending.then(() => true),
			prompt.then(() => false),
		]);
		if (!toolStarted)
			throw new Error(
				`tool did not start: ${JSON.stringify(session.messages)}`,
			);
		const compact = session.compact();
		if (!signal.aborted)
			await new Promise((resolve) =>
				signal.addEventListener("abort", resolve, { once: true }),
			);
		release();
		await compact;
		await prompt;
		const last = session.messages
			.slice()
			.reverse()
			.find((message) => message.role === "assistant");
		const finalText =
			last?.content
				.filter((block) => block.type === "text")
				.map((block) => block.text)
				.join("") ?? "";
		return { savedRetry, retry, calls: core.state.callCount, finalText };
	} finally {
		release();
		session.dispose();
	}
}
