import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
    const database = request.database as string;
    const scenario = request.scenario as string;
    const moduleUrl = pathToFileURL(process.argv[2]);
    const db = await import(new URL("../core/session-control-db.ts", moduleUrl).href);
    const kind = scenario.startsWith("preempt") ? "goal_idle_review" : "approval_review";
    const requestId = db.postSupervisorRequest(database, {
        senderSessionId: "main", projectId: "pi", kind, payload: {}, deadlineAt: "2099-01-01T00:00:00.000Z",
    });
    const claimed = db.claimNextSupervisorRequest(database, "runtime");
    if (scenario === "deadline_settled") claimed.deadlineAt = "2000-01-01T00:00:00.000Z";
    const trace: string[] = [];
    const evaluate = (_prompt: string, signal: AbortSignal) => {
        trace.push("evaluate");
        signal.addEventListener("abort", () => trace.push("abort"), { once: true });
        if (scenario.startsWith("cancel")) {
            db.cancelSupervisorRequest(database, requestId, "main", "fixture cancelled");
            trace.push("cancel");
        }
        if (scenario.startsWith("preempt")) {
            db.postSupervisorRequest(database, {
                senderSessionId: "main", projectId: "pi", kind: "approval_review", payload: {}, deadlineAt: "2099-01-01T00:00:00.000Z",
            });
            trace.push("approval");
        }
        if (scenario === "evaluation_reject") return Promise.reject(new Error("evaluation failed"));
        const response = kind === "approval_review"
            ? { kind: "approve", reason: "ok" }
            : { kind: "continue", reason: "ok", instructions: "Continue." };
        if (scenario.endsWith("pending")) {
            return new Promise(resolve => signal.addEventListener("abort", () => {
                setTimeout(() => {
                    trace.push(`cleanup:${db.readSupervisorRequest(database, requestId).status}`);
                    resolve(response);
                }, 20);
            }, { once: true }));
        }
        return Promise.resolve(response);
    };
    const run = api.runSupervisorRequest as (input: unknown) => Promise<string>;
    let outcome: string | null = null;
    let error: string | null = null;
    try {
        outcome = await run({ controlDbPath: database, evaluate, request: claimed, pollIntervalMs: 1 });
    } catch (failure) {
        error = (failure as Error).message;
    }
    const stored = db.readSupervisorRequest(database, requestId);
    return { outcome, error, status: stored.status, response: stored.response ?? null, trace };
}).catch((error: unknown) => {
    process.stderr.write(`supervisor runtime oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
