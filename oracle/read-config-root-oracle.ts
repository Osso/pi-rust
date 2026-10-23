import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = { createAgentConfigReadToolDefinition(cwd: string, root: string): {
    execute(id: string, args: {path: string}): Promise<unknown>;
} };
void runOracle(async (module, request) => {
    const api = module as unknown as Api;
    const results = [];
    for (const value of requireCases(request)) {
        const input = value as { path: string; root: string };
        try {
            const definition = api.createAgentConfigReadToolDefinition(String(request.cwd), input.root);
            results.push({ value: await definition.execute("root-read", {path: input.path}) });
        } catch (error) {
            results.push({ error: (error as Error).message });
        }
    }
    return results;
});
