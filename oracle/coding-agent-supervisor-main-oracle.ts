import { runOracle } from "./coding-agent-utils-oracle-support.ts";

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
    throw new Error(`Unknown supervisor main operation: ${request.operation}`);
}).catch((error: unknown) => {
    process.stderr.write(`supervisor main oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
