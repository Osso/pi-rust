import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
async function main() {
    const root = process.argv[2];
    const { registerAgentViewerTools } = await import(pathToFileURL(join(root, "extensions/agent-viewer/src/runtime.ts")).href);
    const { MultiAgentStore } = await import(pathToFileURL(join(root, "src/core/multi-agent-store.ts")).href);
    for await (const line of createInterface({ input: process.stdin })) {
        const request = JSON.parse(line);
        const store = new MultiAgentStore();
        for (const agent of request.agents) store.publishLifecycleCoordinatorSnapshot(agent);
        let tool: any;
        registerAgentViewerTools({ registerTool: (definition: any) => { tool = definition; } }, { store });
        try {
            const result = await tool.execute("view", request.params, undefined, undefined, { controlDbPath: request.db });
            console.log(JSON.stringify({ content: result.content, details: result.details ?? null, isError: result.isError ?? null }));
        } catch (error) { console.log(JSON.stringify({ error: String(error) })); }
    }
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
