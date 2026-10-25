import { runOracle } from "./coding-agent-utils-oracle-support.ts";
import { existsSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";
import { pathToFileURL } from "node:url";
import type { OracleRequest } from "./coding-agent-utils-oracle-support.ts";

async function processRequest(api: Record<string, unknown>, request: OracleRequest) {
    const moduleUrl = pathToFileURL(process.argv[2]);
    const db = await import(new URL("../core/session-control-db.ts", moduleUrl).href);
    const database = request.database as string;
    const id = db.postSupervisorRequest(database, {
        senderSessionId: "main", projectId: "pi", kind: request.kind, payload: {}, deadlineAt: "2099-01-01T00:00:00.000Z",
    });
    const claimed = db.claimNextSupervisorRequest(database, "runtime");
    if (request.blockCompletion) {
        const sqlite = await import(new URL("../core/sqlite.ts", moduleUrl).href);
        const connection = sqlite.createSqliteDatabase(database);
        connection.exec("CREATE TRIGGER block_complete BEFORE UPDATE OF status ON supervisor_requests WHEN NEW.status = 'completed' BEGIN SELECT RAISE(ABORT, 'blocked completion'); END");
        connection.close();
    }
    const branch: unknown[] = [];
    const calls: unknown[] = [];
    const feedback: unknown[] = [];
    const reviewed: string[] = [];
    const session = {
        sessionId: "resident-fixture",
        sessionManager: { getBranch: () => branch, getLeafId: () => branch.length ? "reply" : null },
        getContextUsage: () => ({ percent: request.percent }),
        compact: async (content: string) => { calls.push({ kind: "compact", content }); },
        prompt: async (content: string) => {
            calls.push({ kind: "prompt", content });
            branch.push(request.entry);
        },
        abort: async () => { calls.push({ kind: "abort" }); },
        sendCustomMessage: async (message: unknown) => {
            feedback.push({ message, status: db.readSupervisorRequest(database, id).status });
            if (request.feedbackError) throw new Error("feedback failed");
        },
    };
    const evaluate = async (instructions: string, signal: AbortSignal) => {
        reviewed.push(instructions);
        if (request.reviewError) throw new Error("review failed");
        if (request.cancelReview) {
            db.cancelSupervisorRequest(database, id, "main", "fixture cancelled");
            return new Promise((_resolve, reject) => signal.addEventListener("abort", () => reject(new Error("review cancelled")), { once: true }));
        }
        return request.decision;
    };
    const createReviewer = api.createSupervisorInstructionReviewer as (evaluate: unknown) => unknown;
    const reviewer = request.defaultReviewer ? undefined : createReviewer(evaluate);
    const handleRequest = api.processSupervisorRequest as (...args: unknown[]) => Promise<void>;
    let processError: string | null = null;
    try { await handleRequest(database, claimed, session, reviewer); }
    catch (error) { processError = (error as Error).message; }
    const stored = db.readSupervisorRequest(database, id);
    return { status: stored.status, response: stored.response ?? null, feedback, calls, reviewed, processError };
}

async function evaluateInstructions(api: Record<string, unknown>, request: OracleRequest) {
    const compat = await import(new URL("../../../ai/src/compat.ts", pathToFileURL(process.argv[2])).href);
    const responses = [...request.responses as Record<string, unknown>[]];
    const calls: unknown[] = [];
    const controller = new AbortController();
    if (request.aborted) controller.abort();
    const started = Date.now();
    compat.registerApiProvider({
        api: "supervisor-review-fixture",
        stream: () => { throw new Error("unexpected raw stream"); },
        streamSimple: (model: { id: string; provider: string; api: string }, context: { messages: { timestamp: number }[] }, options: Record<string, unknown>) => {
            for (const message of context.messages) {
                if (!Number.isInteger(message.timestamp) || message.timestamp < started || message.timestamp > Date.now()) {
                    throw new Error("invalid review timestamp");
                }
                message.timestamp = 0;
            }
            const signal = options.signal as AbortSignal | undefined;
            calls.push({ model: { id: model.id, provider: model.provider, api: model.api }, context, options: {
                apiKey: options.apiKey ?? null, env: options.env ?? null, headers: options.headers ?? null,
                maxRetries: options.maxRetries ?? null, maxRetryDelayMs: options.maxRetryDelayMs ?? null,
                reasoning: options.reasoning ?? null, timeoutMs: options.timeoutMs ?? null,
                websocketConnectTimeoutMs: options.websocketConnectTimeoutMs ?? null,
                signal: { present: !!signal, aborted: signal?.aborted ?? null },
            } });
            const response = responses.shift();
            if (!response) throw new Error("unexpected fixture provider call");
            if (response.throw) throw new Error(response.throw as string);
            if (request.abortDuringStream) controller.abort();
            return { result: async () => ({
                role: "assistant", api: model.api, provider: model.provider, model: model.id,
                usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0,
                    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
                timestamp: 0, ...response,
            }) };
        },
    }, "supervisor-review-test");
    try {
        const create = api.createSupervisorInstructionEvaluator as (agentDir: string) => (instructions: string, signal: AbortSignal) => Promise<unknown>;
        try {
            const result = await create(request.agentDir as string)(request.instructions as string, controller.signal);
            return { result: result ?? null, error: null, calls };
        } catch (error) {
            return { result: null, error: (error as Error).message, calls };
        }
    } finally {
        compat.unregisterApiProviders("supervisor-review-test");
    }
}

async function waitForServiceState(predicate: () => boolean) {
    const deadline = Date.now() + 15000;
    while (!predicate()) {
        if (Date.now() >= deadline) throw new Error("service fixture timed out");
        await new Promise((resolve) => setTimeout(resolve, 10));
    }
}

async function serviceLifecycle(api: Record<string, unknown>, request: OracleRequest) {
    const moduleUrl = pathToFileURL(process.argv[2]);
    const compat = await import(new URL("../../../ai/src/compat.ts", moduleUrl).href);
    const db = await import(new URL("../core/session-control-db.ts", moduleUrl).href);
    const events = await import(new URL("../../../ai/src/utils/event-stream.ts", moduleUrl).href);
    const wake = await import(new URL("./request-wake.ts", moduleUrl).href);
    const database = db.getControlDbPath();
    const calls: string[] = [];
    const response = { kind: "continue", instructions: "Evidence is available.", reason: "bounded" };
    compat.registerApiProvider({
        api: "supervisor-review-fixture",
        stream: () => { throw new Error("unexpected raw stream"); },
        streamSimple: (model: { api: string; provider: string; id: string }, context: { systemPrompt?: string }) => {
            const review = context.systemPrompt?.startsWith("You are Pi's stateless Supervisor instruction veto gate.");
            calls.push(review ? "review" : "request");
            const message = {
                role: "assistant", api: model.api, provider: model.provider, model: model.id, timestamp: 0,
                content: review ? [{ type: "text", text: "ACCEPT" }] : [{ type: "toolCall", id: "decision", name: "supervisor_response", arguments: response }],
                stopReason: review ? "stop" : "toolUse",
                usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0,
                    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
            };
            const stream = events.createAssistantMessageEventStream();
            stream.push({ type: "start", partial: message });
            stream.push({ type: "done", reason: message.stopReason, message });
            stream.end();
            return stream;
        },
    }, "supervisor-service-test");
    let finished = false;
    let error: string | null = null;
    const run = api.runSupervisorService as () => Promise<void>;
    const running = run().catch((failure) => { error = failure.message; }).finally(() => { finished = true; });
    try {
        await waitForServiceState(() => finished || existsSync(`${database}.supervisor-console.sock`));
        if (finished) throw new Error(`service startup failed: ${error}`);
        if (!existsSync(`${database}.supervisor.sock`)) throw new Error("wake socket did not start");
        const id = db.postSupervisorRequest(database, { senderSessionId: "main", projectId: "fixture", kind: "goal_idle_review",
            payload: { objective: "Verify the fixture." }, deadlineAt: "2099-01-01T00:00:00.000Z" });
        if (request.blockCompletion) {
            const sqlite = await import(new URL("../core/sqlite.ts", moduleUrl).href);
            const connection = sqlite.createSqliteDatabase(database);
            connection.exec("CREATE TRIGGER block_complete BEFORE UPDATE OF status ON supervisor_requests WHEN NEW.status = 'completed' BEGIN SELECT RAISE(ABORT, 'blocked completion'); END");
            connection.close();
        }
        wake.notifySupervisorRequest(database);
        await waitForServiceState(() => finished || db.readSupervisorRequest(database, id).status === "completed");
        if (!finished) process.kill(process.pid, "SIGTERM");
        await running;
        const stored = db.readSupervisorRequest(database, id);
        return { error, calls, status: stored.status, response: stored.response ?? null,
            consoleClosed: !existsSync(`${database}.supervisor-console.sock`),
            wakeClosed: !existsSync(`${database}.supervisor.sock`) };
    } finally {
        if (!finished) { process.kill(process.pid, "SIGTERM"); await running; }
        compat.unregisterApiProviders("supervisor-service-test");
    }
}

void runOracle(async (api, request) => {
    if (request.operation === "serviceLifecycle") return serviceLifecycle(api, request);
    if (request.operation === "evaluateInstructions") return evaluateInstructions(api, request);
    if (request.operation === "consoleSnapshot") {
        const create = api.createSupervisorConsoleSnapshot as (input: unknown) => {
            identity: { pid: number; executable: string; entrypoint?: string; instanceId: string };
        };
        const input = { cwd: request.cwd, generation: request.generation, managedBy: request.managedBy,
            session: { sessionId: request.sessionId, sessionManager: { getBranch: () => [] } } };
        const snapshot = create(input);
        if (snapshot.identity.pid !== process.pid || snapshot.identity.executable !== process.execPath || snapshot.identity.entrypoint !== process.argv[1]) {
            throw new Error("snapshot does not identify the running process");
        }
        if (!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(snapshot.identity.instanceId)) {
            throw new Error("snapshot instance is not a v4 UUID");
        }
        if (create(input).identity.instanceId !== snapshot.identity.instanceId) throw new Error("unstable snapshot instance");
        return { ...snapshot, identity: { ...snapshot.identity, pid: 0, executable: "<executable>",
            entrypoint: "<entrypoint>", instanceId: "<uuid>" } };
    }
    if (request.operation === "processRequest") return processRequest(api, request);
    if (request.operation === "modelRequirements") {
        const create = api.createSupervisorInstructionEvaluator as (agentDir: string) => unknown;
        try {
            create(request.agentDir as string);
            return { available: true, error: null };
        } catch (error) {
            return { available: false, error: (error as Error).message };
        }
    }
    if (request.operation === "openSession") {
        const open = api.openSupervisorSession as (agentDir: string, kbDir: string, database: string) => {
            getSessionFile(): string | undefined; getSessionId(): string; getCwd(): string;
        };
        const session = open(request.agentDir as string, request.kbDir as string, request.database as string);
        const file = session.getSessionFile();
        if (!file) throw new Error("Expected resident session file");
        return {
            files: readdirSync(join(request.agentDir as string, "supervisor-sessions")).sort(),
            file: basename(file), id: session.getSessionId(), cwd: session.getCwd(),
        };
    }
    throw new Error(`Unknown supervisor main operation: ${request.operation}`);
}).catch((error: unknown) => {
    process.stderr.write(`supervisor main oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
