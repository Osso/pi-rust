import { runOracle } from "./coding-agent-utils-oracle-support.ts";
import { readdirSync } from "node:fs";
import { basename, join } from "node:path";

void runOracle((api, request) => {
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
