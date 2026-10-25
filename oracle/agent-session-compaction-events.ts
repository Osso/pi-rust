import { dirname, resolve } from "node:path";
import { fauxAssistantMessage } from "@earendil-works/pi-ai/compat";

async function bindSavedEventFixture(
	context,
	{ cancel, order, saved, started, gate },
) {
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
	let preparation;
	const extension = await loadExtensionFromFactory(
		(api) => {
			api.on("compaction", (event) => {
				preparation = event.preparation;
				return cancel
					? { cancel: true }
					: {
							compaction: {
								summary: "saved summary",
								firstKeptEntryId: preparation.firstKeptEntryId,
								tokensBefore: preparation.tokensBefore,
								source: { type: "local", provider: "faux", model: "faux-1" },
								details: { origin: "extension" },
							},
						};
			});
			api.on("session_compact", async (event) => {
				order.push("session_compact:start");
				saved.push(event);
				started();
				await gate;
				order.push("session_compact:end");
			});
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
	session.setActiveToolsByName([]);
	return () => preparation;
}

function normalizeSavedEvent(event) {
	const wire = JSON.parse(JSON.stringify(event));
	const entry = wire.compactionEntry;
	for (const field of ["id", "parentId", "timestamp", "firstKeptEntryId"]) {
		if (typeof entry[field] === "string") entry[field] = `<${field}>`;
	}
	if (typeof entry.durationMs === "number") entry.durationMs = "<durationMs>";
	return wire;
}

export async function runCompactionEventsScenario(operation, context) {
	const { session, sessionManager, settingsManager, agent, model } = context;
	const auto = operation.startsWith("auto-");
	const cancel = operation.includes("cancel");
	sessionManager.appendMessage({
		role: "user",
		content: "older message to compact",
		timestamp: 0,
	});
	sessionManager.appendMessage(
		fauxAssistantMessage("assistant response to compact"),
	);
	for (let i = 0; i < 4; i++)
		sessionManager.appendMessage({
			role: "user",
			content: `newer request ${i}`,
			timestamp: i + 1,
		});
	sessionManager.appendMessage(fauxAssistantMessage("latest answer"));
	agent.state.messages = sessionManager.buildSessionContext().messages;
	settingsManager.setCompactionEnabled(false);
	const order = [];
	const saved = [];
	const endCompacting = [];
	let signalStarted;
	let release;
	const started = new Promise((resolve) => {
		signalStarted = resolve;
	});
	const gate = new Promise((resolve) => {
		release = resolve;
	});
	try {
		const readPreparation = await bindSavedEventFixture(context, {
			cancel,
			order,
			saved,
			started: signalStarted,
			gate,
		});
		session.subscribe((event) => {
			if (event.type === "compaction_end") {
				order.push("compaction_end");
				endCompacting.push(session.isCompacting);
			}
		});
		let anchor;
		if (auto) {
			agent.state.model = {
				...model,
				contextWindow: 1000,
				autoCompactionThreshold: 900,
			};
			anchor = fauxAssistantMessage("anchor");
			anchor.usage.input = 800;
			anchor.usage.totalTokens = 800;
			sessionManager.appendMessage(anchor);
			agent.state.messages = sessionManager.buildSessionContext().messages;
			await session.sendCustomMessage({
				customType: "tail",
				content: "x".repeat(800),
				display: false,
			});
		}
		let error = null;
		const task = (
			auto
				? agent.prepareNextTurnWithContext({
						message: anchor,
						toolResults: [],
						newMessages: [],
						context: {
							systemPrompt: agent.state.systemPrompt,
							messages: agent.state.messages.slice(),
							tools: agent.state.tools.slice(),
						},
					})
				: session.compact()
		).catch((cause) => {
			error = cause.message;
		});
		const handlerStarted = await Promise.race([
			started.then(() => true),
			task.then(() => false),
		]);
		const settledBeforeRelease = handlerStarted
			? await Promise.race([
					task.then(() => true),
					new Promise((resolve) => setTimeout(() => resolve(false), 100)),
				])
			: true;
		release();
		await task;
		const entries = sessionManager.getEntries();
		const preparation = readPreparation();
		const snapshots = saved.map((event) => ({
			entryType: event.compactionEntry.type,
			summary: event.compactionEntry.summary,
			fromExtension: event.fromExtension,
			fromHook: event.compactionEntry.fromHook,
			reason: event.reason,
			willRetry: event.willRetry,
			entryMatchesSaved: entries.some(
				(entry) => entry.id === event.compactionEntry.id,
			),
			matchesHook:
				event.compactionEntry.firstKeptEntryId ===
					preparation.firstKeptEntryId &&
				event.compactionEntry.tokensBefore === preparation.tokensBefore,
			durationPresent: typeof event.compactionEntry.durationMs === "number",
			details: event.compactionEntry.details,
		}));
		return {
			handlerStarted,
			settledBeforeRelease,
			order,
			endCompacting,
			afterCompacting: session.isCompacting,
			error,
			snapshots,
			wires: saved.map(normalizeSavedEvent),
			rawWires: saved.map((event) => JSON.parse(JSON.stringify(event))),
		};
	} finally {
		release();
		session.dispose();
	}
}
