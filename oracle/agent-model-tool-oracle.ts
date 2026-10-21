// Fixture-only execution of the frozen tool factory and public store/DB APIs.
import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
    const source = process.argv[2];
    if (!source) throw new Error("agent-model oracle requires frozen source module");
    const tool = await import(pathToFileURL(source).href);
    const src = join(dirname(source), "../../../src");
    const { MultiAgentStore } = await import(pathToFileURL(join(src, "core/multi-agent-store.ts")).href);
    const db = await import(pathToFileURL(join(src, "core/session-control-db.ts")).href);
    
    function metadata(definition: any) {
        return { name: definition.name, label: definition.label, description: definition.description,
            promptGuidelines: definition.promptGuidelines, approvalRequired: definition.approvalRequired,
            parameters: definition.parameters };
    }
    
    async function createLiveTarget(store: any, agentId: string, cwd: string) {
        const load = (name: string) => import(pathToFileURL(join(src, `core/${name}.ts`)).href);
        const { AuthStorage } = await load("auth-storage");
        const { ModelRegistry } = await load("model-registry");
        const { SessionManager } = await load("session-manager");
        const { SettingsManager } = await load("settings-manager");
        const { createAgentSession } = await load("sdk");
        const { createExtensionRuntime } = await load("extensions/loader");
        const authStorage = AuthStorage.inMemory({ faux: { type: "api_key", key: "fixture" } });
        const modelRegistry = ModelRegistry.create(authStorage, join(cwd, "models.json"));
        modelRegistry.registerProvider("faux", {
            api: "openai-completions", baseUrl: "http://127.0.0.1:1", apiKey: "fixture",
            models: ["old", "new", "plain"].map(id => ({ id, name: id, reasoning: id !== "plain", input: ["text"],
                cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 100000, maxTokens: 4096 })),
        });
        const resourceLoader = {
            getExtensions: () => ({ extensions: [], errors: [], runtime: createExtensionRuntime() }),
            getSkills: () => ({ skills: [], diagnostics: [] }), getPrompts: () => ({ prompts: [], diagnostics: [] }),
            getThemes: () => ({ themes: [], diagnostics: [] }), getAgentsFiles: () => ({ agentsFiles: [] }),
            getRulesFiles: () => ({ rulesFiles: [] }), getRulesContent: () => undefined,
            getSystemPrompt: () => undefined, getAppendSystemPrompt: () => [], extendResources: () => {}, reload: async () => {},
        };
        const { session } = await createAgentSession({ cwd, agentDir: cwd, authStorage, modelRegistry,
            model: modelRegistry.find("faux", "old"), thinkingLevel: "low", resourceLoader,
            sessionManager: SessionManager.inMemory(cwd), settingsManager: SettingsManager.inMemory(),
            multiAgentStore: store, multiAgentAgentId: agentId, disableRuntimeCoordinationInbound: true,
        });
        return session;
    }

    async function execute(request: any) {
        if (request.op === "read") return db.readMultiAgentAgent(request.db, request.session, request.agentId);
        if (request.op === "create") {
            for (const agent of request.agents) db.bootstrapMultiAgentAgent(request.db, request.session, agent.id, agent);
            const admitted = db.createMultiAgentChildWithRuntimeOwnership(request.db, request.input);
            if (!admitted.ok) throw new Error(JSON.stringify(admitted));
            return admitted;
        }
        const store = new MultiAgentStore();
        for (const agent of request.agents ?? []) store.publishLifecycleCoordinatorSnapshot(agent);
        if (request.selected) store.selectAgentView(request.selected);
        const sessions = new Map();
        // Boundary fixture for source validation/errors; real Rust live sessions supply runtime proof.
        for (const id of request.live ?? []) sessions.set(id, {});
        let target: any;
        if (request.op === "emptyModel") {
            sessions.set(request.params.agentId, {
                model: undefined, thinkingLevel: "off", modelRegistry: {}, scopedModels: [],
                setModel: async () => { throw new Error("unexpected model selection in empty-model fixture"); },
                setThinkingLevel(level: string) { this.thinkingLevel = level; },
            });
        }
        if (request.op === "success") {
            target = await createLiveTarget(store, request.params.agentId, request.cwd);
            sessions.set(request.params.agentId, target);
        }
        let definition: any;
        tool.registerSetAgentModelTool({ registerTool: (value: any) => { definition = value; } }, store, sessions);
        if (request.op === "metadata") return metadata(definition);
        if (request.op === "validate") {
            const { validateToolArguments } = await import(pathToFileURL(join(src, "../../ai/src/utils/validation.ts")).href);
            return request.cases.map((params: any) => {
                try { return { arguments: validateToolArguments(definition, { id: "fixture", name: definition.name, arguments: params }) }; }
                catch (error) { return { error: (error as Error).message }; }
            });
        }
        try {
            const result = await definition.execute("fixture-call", request.params);
            return { result, selected: store.getSelectedAgentId() ?? null, ...(target ? { model: store.getAgent(request.params.agentId).model } : {}) };
        } catch (error) {
            return { error: (error as Error).message, selected: store.getSelectedAgentId() ?? null };
        } finally {
            target?.dispose();
        }
    }
    
    for await (const line of createInterface({ input: process.stdin })) {
        try { process.stdout.write(`${JSON.stringify(await execute(JSON.parse(line)))}\n`); }
        catch (error) { process.stdout.write(`${JSON.stringify({ fixtureError: (error as Error).message })}\n`); }
    }
}

void main().catch((error: unknown) => { console.error(error); process.exitCode = 1; });
