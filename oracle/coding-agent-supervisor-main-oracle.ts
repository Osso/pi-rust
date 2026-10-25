import { runOracle } from "./coding-agent-utils-oracle-support.ts";
import { readdirSync } from "node:fs";
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

void runOracle(async (api, request) => {
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
