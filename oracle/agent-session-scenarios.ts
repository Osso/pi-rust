// Named fixtures around the actual frozen AgentSession; no source rewriting.
import { fauxAssistantMessage } from "@earendil-works/pi-ai/compat";
import { runPromptLifecycleScenario } from "./agent-session-prompt-lifecycle.ts";
import { runInputImagesScenario } from "./agent-session-input-images.ts";
import { runCompactionEventsScenario } from "./agent-session-compaction-events.ts";
import { runManualToolCompaction } from "./agent-session-manual-tool.ts";

export async function runAgentSessionScenario(request, context) {
	if (request.operation === "thinking-cycle-unlisted") {
		context.agent.state.thinkingLevel = "xhigh";
		const before = context.session.thinkingLevel;
		const available = context.session.getAvailableThinkingLevels();
		const next = context.session.cycleThinkingLevel();
		const result = {
			before,
			available,
			next,
			current: context.session.thinkingLevel,
		};
		context.session.dispose();
		return result;
	}
	if (request.operation === "manual-tool-compaction")
		return runManualToolCompaction(context);
	if (
		[
			"manual-compact-events",
			"auto-compact-events",
			"manual-cancel-events",
			"auto-cancel-events",
		].includes(request.operation)
	)
		return runCompactionEventsScenario(request.operation, context);
	if (["input-images", "input-image-propagation"].includes(request.operation))
		return runInputImagesScenario(
			context,
			request.operation === "input-image-propagation",
		);
	if (request.operation === "prompt-steering-lifecycle")
		return runPromptLifecycleScenario(context);
	if (request.operation === "runner-identity")
		return runRunnerIdentity(request, context);
	if (request.operation === "export-error")
		return runExportError(request, context);
	if (request.operation === "navigation-empty-summary")
		return runEmptyNavigation(request, context);
	if (request.operation === "navigation-auth")
		return runNavigationAuth(request, context);
	if (
		[
			"preflight-compaction",
			"compaction-lock",
			"repeat-overflow",
			"threshold-trailing",
			"next-turn-threshold",
			"auto-abort-hook",
			"auto-duration-hook",
			"background-events",
			"background-disabled",
			"background-cancel-wait",
			"background-ancestor",
		].includes(request.operation)
	)
		return runCompactionScenario(request, context);
	if (["idle-steer", "navigation-streaming"].includes(request.operation))
		return runIdleSteer(request, context);
	if (request.operation === "quota-fallback")
		return runQuotaFallback(request, context);
	if (request.operation === "cycle-models")
		return runModelCycle(request, context);
	throw new Error(
		"Unknown agent-session oracle scenario: " + request.operation,
	);
}

async function runRunnerIdentity(_request, context) {
	const {
		session,
		modules,
		dir,
		sessionManager,
		modelRegistry,
		settingsManager,
	} = context;

	const runner = session.extensionRunner;
	const alias = session.extensionRunner;
	const fresh = new modules.ExtensionRunner(
		[],
		session.resourceLoader.getExtensions().runtime,
		dir,
		sessionManager,
		modelRegistry,
		settingsManager,
	);
	session.dispose();
	return { aliasSame: runner === alias, freshSame: runner === fresh };
}

async function runExportError(request, context) {
	const { session } = context;

	try {
		session.exportToJsonl(request.outputPath);
		return { success: true };
	} catch (error) {
		return {
			name: error.name,
			message: error.message,
			...Object.fromEntries(
				Object.entries(error).filter(([key]) => key !== "stack"),
			),
		};
	} finally {
		session.dispose();
	}
}

async function runEmptyNavigation(_request, context) {
	const { sessionManager, agent, session } = context;

	const first = sessionManager.appendMessage({
		role: "user",
		content: "first request",
		timestamp: 0,
	});
	sessionManager.appendMessage(fauxAssistantMessage("old answer"));
	agent.state.messages = sessionManager.buildSessionContext().messages;
	const runner = session.extensionRunner;
	const emit = runner.emit.bind(runner);
	runner.hasHandlers = (type) => type === "session_before_tree";
	runner.emit = async (event) =>
		event.type === "session_before_tree"
			? { summary: { summary: "" }, label: "" }
			: emit(event);
	const result = await session.navigateTree(first, {
		summarize: true,
		label: "",
	});
	const entries = sessionManager.getEntries();
	session.dispose();
	return {
		hasSummary: result.summaryEntry !== undefined,
		atRoot: sessionManager.getLeafId() === null,
		summaries: entries.filter((entry) => entry.type === "branch_summary")
			.length,
		labels: entries.filter((entry) => entry.type === "label").length,
	};
}

async function runNavigationAuth(_request, context) {
	const { sessionManager, agent, model, session } = context;

	const first = sessionManager.appendMessage({
		role: "user",
		content: "first request",
		timestamp: 0,
	});
	sessionManager.appendMessage(fauxAssistantMessage("old answer"));
	agent.state.model = { ...model, provider: "unconfigured-provider" };
	agent.state.messages = sessionManager.buildSessionContext().messages;
	try {
		await session.navigateTree(first, { summarize: true });
		return { success: true };
	} catch (error) {
		return { error: error.message };
	} finally {
		session.dispose();
	}
}

async function runCompactionScenario(request, context) {
	const { sessionManager, agent, settingsManager, session } = context;

	sessionManager.appendMessage({
		role: "user",
		content: "older message to compact",
		timestamp: 0,
	});
	sessionManager.appendMessage(
		fauxAssistantMessage("assistant response to compact"),
	);
	for (let i = 0; i < 4; i++) {
		sessionManager.appendMessage({
			role: "user",
			content: `newer request ${i}`,
			timestamp: i + 1,
		});
	}
	sessionManager.appendMessage(fauxAssistantMessage("latest answer"));
	agent.state.messages = sessionManager.buildSessionContext().messages;
	settingsManager.setCompactionEnabled(false);
	const runner = session.extensionRunner;
	const emit = runner.emit.bind(runner);
	if (
		[
			"background-events",
			"background-disabled",
			"background-cancel-wait",
			"background-ancestor",
		].includes(request.operation)
	)
		return runBackgroundScenario(request, { ...context, runner, emit });
	if (
		[
			"threshold-trailing",
			"next-turn-threshold",
			"auto-abort-hook",
			"auto-duration-hook",
		].includes(request.operation)
	)
		return runThresholdScenario(request, { ...context, runner, emit });
	if (request.operation === "repeat-overflow")
		return runRepeatedOverflow(request, { ...context, runner, emit });
	if (request.operation === "compaction-lock")
		return runCompactionLock(request, { ...context, runner, emit });
	runner.hasHandlers = (type) => type === "session_before_compact";
	runner.emit = async (event) =>
		event.type === "session_before_compact"
			? {
					compaction: {
						summary: "obsolete preflight summary",
						firstKeptEntryId: event.preparation.firstKeptEntryId,
						tokensBefore: 1,
					},
				}
			: emit(event);
	try {
		const result = await session.compact("keep decisions");
		return { summary: result.summary };
	} catch (error) {
		return { error: error.message };
	} finally {
		session.dispose();
	}
}

async function runIdleSteer(request, context) {
	const { sessionManager, agent, core, session } = context;

	sessionManager.appendMessage({
		role: "user",
		content: "earlier request",
		timestamp: 0,
	});
	agent.state.messages = sessionManager.buildSessionContext().messages;
	let release;
	let markStarted;
	const gate = new Promise((resolve) => {
		release = resolve;
	});
	const started = new Promise((resolve) => {
		markStarted = resolve;
	});
	core.setResponses([
		async () => {
			markStarted();
			await gate;
			return fauxAssistantMessage("steered");
		},
	]);
	const steering = session.steer("idle steer");
	await started;
	let navigation;
	if (request.operation === "navigation-streaming") {
		try {
			navigation = await session.navigateTree(sessionManager.getLeafId());
		} catch (error) {
			navigation = { error: error.message };
		}
	}
	const returnedBeforeResponse = await Promise.race([
		steering.then(() => true),
		new Promise((resolve) => setTimeout(() => resolve(false), 100)),
	]);
	release();
	await steering;
	await session.abort();
	session.dispose();
	return request.operation === "navigation-streaming"
		? navigation
		: returnedBeforeResponse;
}

async function runQuotaFallback(_request, context) {
	const { session, requestedProviders } = context;

	await session.prompt("quota test");
	const result = { requestedProviders, provider: session.model.provider };
	session.dispose();
	return result;
}

async function runModelCycle(request, context) {
	const { session, model } = context;

	session.setScopedModels(
		request.scopes.map((scope) => ({
			model: { ...model, ...scope },
			thinkingLevel: "high",
		})),
	);
	const results = [];
	for (const direction of request.directions) {
		const result = await session.cycleModel(direction);
		results.push(
			result
				? {
						id: result.model.id,
						name: result.model.name,
						thinkingLevel: result.thinkingLevel,
						isScoped: result.isScoped,
					}
				: null,
		);
	}
	session.dispose();
	return results;
}

async function runBackgroundScenario(request, context) {
	const { runner, session, sessionManager, requestedProviders } = context;

	const events = [];
	let finish;
	let release;
	let markStarted;
	const started = new Promise((resolve) => {
		markStarted = resolve;
	});
	const gate = new Promise((resolve) => {
		release = resolve;
	});
	const settled = new Promise((resolve) => {
		finish = resolve;
	});
	runner.hasHandlers = (type) => type === "compaction";
	runner.emit = async (event) => {
		events.push(event.type);
		if (
			event.type === "background_compaction_start" &&
			request.operation === "background-ancestor"
		) {
			markStarted();
			await gate;
		}
		if (
			event.type === "compaction" &&
			request.operation === "background-ancestor"
		)
			return {
				compaction: {
					summary: "saved background summary",
					firstKeptEntryId: event.preparation.firstKeptEntryId,
					tokensBefore: 1,
				},
			};
		if (
			event.type === "compaction" &&
			request.operation === "background-cancel-wait"
		)
			return { cancel: true };
		if (event.type === "background_compaction_end") {
			finish();
			if (request.operation === "background-cancel-wait") await gate;
		}
	};
	session._startBackgroundCompaction(80000, 100000);
	if (request.operation === "background-ancestor") {
		await started;
		await session.sendCustomMessage({
			customType: "tail",
			content: "status while pending",
			display: false,
		});
		release();
	}
	await settled;
	if (request.operation === "background-ancestor") {
		for (
			let i = 0;
			i < 100 &&
			!sessionManager.getEntries().some((entry) => entry.type === "compaction");
			i++
		)
			await new Promise((resolve) => setTimeout(resolve, 10));
		const result = {
			installed: sessionManager
				.getEntries()
				.some((entry) => entry.type === "compaction"),
			tailPresent: session.messages.some(
				(message) => message.role === "custom" && message.customType === "tail",
			),
			tailText: session.messages.find(
				(message) => message.role === "custom" && message.customType === "tail",
			)?.content,
		};
		session.dispose();
		return result;
	}
	if (request.operation === "background-cancel-wait") {
		const cancelling = session._cancelBackgroundCompaction();
		const completedBeforeEnd = await Promise.race([
			cancelling.then(() => true),
			new Promise((resolve) => setTimeout(() => resolve(false), 100)),
		]);
		release();
		await cancelling;
		session.dispose();
		return { completedBeforeEnd };
	}
	session.dispose();
	return request.operation === "background-disabled"
		? {
				events: events.filter((event) => event !== "compaction"),
				providerCalls: requestedProviders.length,
			}
		: events;
}

async function runThresholdScenario(request, context) {
	const {
		settingsManager,
		agent,
		model,
		sessionManager,
		session,
		runner,
		emit,
		core,
	} = context;

	settingsManager.setRetryEnabled(false);
	agent.state.model = {
		...model,
		contextWindow: 1000,
		autoCompactionThreshold: 900,
	};
	const anchor = fauxAssistantMessage("anchor");
	anchor.usage.input = request.operation === "threshold-trailing" ? 400 : 600;
	anchor.usage.totalTokens = anchor.usage.input;
	sessionManager.appendMessage(anchor);
	agent.state.messages = sessionManager.buildSessionContext().messages;
	await session.sendCustomMessage({
		customType: "tail",
		content: "x".repeat(800),
		display: false,
	});
	const ends = [];
	session.subscribe((event) => {
		if (event.type === "compaction_end")
			ends.push({
				aborted: event.aborted,
				hasResult: event.result !== undefined,
				willRetry: event.willRetry,
				durationPositive: event.result ? event.result.durationMs >= 15 : null,
			});
	});
	let compactions = 0;
	runner.hasHandlers = (type) => type === "compaction";
	runner.emit = async (event) => {
		if (event.type !== "compaction") return emit(event);
		compactions++;
		if (request.operation === "auto-abort-hook") session.abortCompaction();
		if (request.operation === "auto-duration-hook")
			await new Promise((resolve) => setTimeout(resolve, 30));
		return {
			compaction: {
				summary: "saved summary",
				firstKeptEntryId: event.preparation.firstKeptEntryId,
				tokensBefore: 1,
			},
		};
	};
	if (request.operation !== "threshold-trailing") {
		const update = await agent.prepareNextTurnWithContext({
			message: anchor,
			toolResults: [],
			newMessages: [],
			context: {
				systemPrompt: agent.state.systemPrompt,
				messages: agent.state.messages.slice(),
				tools: agent.state.tools.slice(),
			},
		});
		const saved = sessionManager
			.getEntries()
			.filter((entry) => entry.type === "compaction").length;
		session.dispose();
		if (request.operation.startsWith("auto-"))
			return { saved, end: ends.at(-1) };
		return {
			compactions,
			hasSummary: update.context.messages.some(
				(message) => message.role === "compactionSummary",
			),
		};
	}
	core.setResponses([
		fauxAssistantMessage([], { stopReason: "error", errorMessage: "fatal" }),
	]);
	let beforeRun;
	await session.prompt("next request", {
		preflightResult: (accepted) => {
			if (accepted) beforeRun = compactions;
		},
	});
	session.dispose();
	return { compactions, beforeRun };
}

async function runRepeatedOverflow(_request, context) {
	const { settingsManager, runner, emit, core, session } = context;

	settingsManager.setRetryEnabled(false);
	let compactions = 0;
	runner.hasHandlers = (type) => type === "compaction";
	runner.emit = async (event) => {
		if (event.type !== "compaction") return emit(event);
		compactions++;
		return {
			compaction: {
				summary: "saved summary",
				firstKeptEntryId: event.preparation.firstKeptEntryId,
				tokensBefore: 1,
			},
		};
	};
	core.setResponses(
		[
			"maximum context length exceeded",
			"fatal",
			"maximum context length exceeded",
			"fatal",
		].map((errorMessage) =>
			fauxAssistantMessage([], {
				stopReason: "error",
				errorMessage,
				timestamp: 0,
			}),
		),
	);
	await session.prompt("first overflow request");
	await session.prompt("second overflow request");
	session.dispose();
	return { compactions };
}

async function runCompactionLock(_request, context) {
	const {
		sessionManager,
		agent,
		settingsManager,
		runner,
		emit,
		core,
		session,
	} = context;

	sessionManager.appendMessage({
		role: "user",
		content: "unfinished request",
		timestamp: 10,
	});
	agent.state.messages = sessionManager.buildSessionContext().messages;
	settingsManager.setRetryEnabled(false);
	runner.hasHandlers = (type) => type === "compaction";
	runner.emit = async (event) =>
		event.type === "compaction"
			? {
					compaction: {
						summary: "saved summary",
						firstKeptEntryId: event.preparation.firstKeptEntryId,
						tokensBefore: 1,
					},
				}
			: emit(event);
	let release;
	let markStarted;
	const gate = new Promise((resolve) => {
		release = resolve;
	});
	const started = new Promise((resolve) => {
		markStarted = resolve;
	});
	core.setResponses([
		async () => {
			markStarted();
			await gate;
			return fauxAssistantMessage("resumed answer");
		},
	]);
	const compacting = session.compact();
	await started;
	const accepted = await Promise.race([
		session
			.prompt("new input", { streamingBehavior: "followUp" })
			.then(() => true),
		new Promise((resolve) => setTimeout(() => resolve(false), 100)),
	]);
	release();
	await session.abort();
	await compacting;
	session.dispose();
	return accepted;
}
