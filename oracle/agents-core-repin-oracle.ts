import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

type Request = Record<string, unknown>;
type Listener = () => void;
type SettlementStore = {
	getPersistenceTarget(): undefined;
	getAgent(): { lifecycle: string };
	listDescendants(): Array<{ id: string; parentId: string; lifecycle: string }>;
	subscribeAgentUpdates(listener: Listener): Listener;
};
type SettlementModule = {
	waitForActiveDescendants(store: SettlementStore, id: string, signal: AbortSignal): Promise<void>;
};
type RuntimeModule = {
    registerAgentsCoreTools(api: unknown, options: unknown): void;
    createMultiAgentRuntimeHandles(): unknown;
    createProductionChildAgentSessionFactory(options: unknown): (input: unknown) => Promise<unknown>;
    createProductionAttachedSessionFactory(options: unknown): (input: unknown) => Promise<unknown>;
};
type RouteModule = {
    sessionMessageRouteError(db: string | undefined, params: unknown): string | undefined;
    isRemoteSessionMessage(params: unknown): boolean;
};
type ChildSessionModule = { bindProductionChildSession(session: unknown): { dispose(): Promise<void> } };
type ChildResponseModule = {
	readCurrentChildAssistantText(messages: unknown[], previous: ReadonlySet<unknown>): string | undefined;
};
type DetachedModule = {
    isDetachedRuntimeAgent(agent: unknown): boolean;
    readDetachedRuntimeAgentIds(store: unknown, ids: string[]): Set<string>;
    requestDirectDetachedRuntimeCancellations(store: unknown, parentId: string, reason: string): void;
};

async function load<T>(name: string): Promise<T> {
	const source = process.argv[2];
	if (!source) throw new Error("agents-core source directory is required");
	return import(pathToFileURL(resolve(source, name)).href) as Promise<T>;
}

async function runSettlementAbort(cancelling: boolean): Promise<unknown> {
	const { waitForActiveDescendants } = await load<SettlementModule>("descendant-settlement.ts");
	const controller = new AbortController();
	controller.abort();
	const child = { id: "child", parentId: "parent", lifecycle: "running" };
	const listeners = new Set<Listener>();
	const store: SettlementStore = {
		getPersistenceTarget: () => undefined,
		getAgent: () => ({ lifecycle: cancelling ? "cancelling" : "running" }),
		listDescendants: () => [child],
		subscribeAgentUpdates: (listener) => {
			listeners.add(listener);
			return () => { listeners.delete(listener); };
		},
	};
	let settled = false;
	const waiting = waitForActiveDescendants(store, "parent", controller.signal).then(() => { settled = true; });
	await Promise.resolve();
	const settledBeforeUpdate = settled;
	child.lifecycle = "completed";
	for (const listener of listeners) listener();
	await waiting;
	return { settledBeforeUpdate, listenersAfterFinish: listeners.size };
}

async function execute(request: Request): Promise<unknown> {
	switch (request.op) {
		case "settlementAbort": return runSettlementAbort(request.cancelling === true);
        case "factoryOptions": {
            const api = await load<RuntimeModule>("runtime.ts");
            const registry = {find: () => undefined};
            const manager = {
                getSessionFile: () => undefined, getSessionId: () => "parent",
                getSessionDir: () => request.sessionDir, getLeafId: () => undefined,
                getBranch: () => [], persistForRecovery: () => {},
            };
            let captured: unknown;
            const options = {
                agentDir: request.agentDir,
                createSessionManager: () => manager,
                createSession: async (input: Record<string, unknown>) => {
                    captured = {
                        excludeTools: input.excludeTools, tools: input.tools ?? null,
                        customTools: (input.customTools as Array<{name: string}> | undefined)?.map(tool => tool.name) ?? [],
                        parentRegistry: input.modelRegistry === registry,
                        settingsOverride: input.settingsManager !== undefined,
                    };
                    return {session: {
                        sessionManager: input.sessionManager, bindExtensions: async () => {},
                        setModel: async () => {}, setThinkingLevel: () => {}, cycleModel: async () => {},
                        prompt: async () => {},
                    }};
                },
            };
            const ctx = {
                cwd: request.cwd, controlDbPath: request.controlDbPath, modelRegistry: registry,
                sessionManager: manager,
                settingsManager: {getAgentProfile: () => ({tools: request.tools})},
            };
            const agent = {id: "child", agentType: request.agentType, cwd: request.cwd, permission: {policy: "never"}};
            if (request.attached) {
                await api.createProductionAttachedSessionFactory(options)({agent, ctx, sessionPath: request.sessionPath});
            } else {
                await api.createProductionChildAgentSessionFactory(options)({agent, ctx, context: "fresh", prompt: "task"});
            }
            return captured;
        }
        case "shutdownScope": {
            const api=await load<RuntimeModule>("runtime.ts");
            const hooks=new Map<string,(event:unknown,ctx:unknown)=>unknown>();
            let invalidations=0;
            api.registerAgentsCoreTools({on:(name:string,handler:(event:unknown,ctx:unknown)=>unknown)=>hooks.set(name,handler),registerTool:()=>{},registerCommand:()=>{}},{store:{invalidateInFlightDispatches:()=>{invalidations+=1;}},runtimeHandles:api.createMultiAgentRuntimeHandles()});
            const shutdown=hooks.get("session_shutdown");
            if (!shutdown) throw new Error("missing source shutdown hook");
            await shutdown({reason:request.reason},{multiAgentAgentId:request.child ? "child" : undefined,multiAgentRequiresAgentId:request.child===true});
            return {invalidations};
        }
        case "sessionRoute": {
            const api=await load<RouteModule>("session-message-route.ts");
            return {error:api.sessionMessageRouteError(typeof request.db === "string" ? request.db : undefined,request.params) ?? null,remote:api.isRemoteSessionMessage(request.params)};
        }
        case "childSessionDisposal": {
            const {bindProductionChildSession} = await load<ChildSessionModule>("child-session.ts");
            const events: string[] = [];
            const session = {
                sessionManager: {getBranch: () => []},
                extensionRunner: {emit: async () => {
                    events.push("shutdown");
                    if (typeof request.shutdownError === "string") throw new Error(request.shutdownError);
                }},
                dispose: async () => {
                    events.push("dispose");
                    if (typeof request.disposeError === "string") throw new Error(request.disposeError);
                },
                setModel: async () => {}, setThinkingLevel: () => {}, cycleModel: async () => {},
                prompt: async () => {},
            };
            let failure: string | null = null;
            try { await bindProductionChildSession(session).dispose(); }
            catch (error) { failure = error instanceof Error ? error.message : String(error); }
            return {events, error: failure};
        }
		case "childResponse": {
			const { readCurrentChildAssistantText } = await load<ChildResponseModule>("child-response.ts");
			const messages = request.messages as unknown[];
			const previous = new Set(messages.slice(0, Number(request.previousCount ?? 0)));
			return readCurrentChildAssistantText(messages, previous) ?? null;
		}
        case "detachedMemory": {
            const api = await load<DetachedModule>("detached-runtime-cancellation.ts");
            const store = { getPersistenceTarget: () => undefined, listAgents: () => request.agents };
            const ids = [...api.readDetachedRuntimeAgentIds(store, request.ids as string[])];
            let directError: string | null = null;
            try { api.requestDirectDetachedRuntimeCancellations(store, String(request.parent), "cleanup"); }
            catch (error) { directError = error instanceof Error ? error.message : String(error); }
            return { ids, directError };
        }
		case "isDetached": {
			const { isDetachedRuntimeAgent } = await load<DetachedModule>("detached-runtime-cancellation.ts");
			return isDetachedRuntimeAgent(request.agent);
		}
		default: throw new Error(`Unknown operation ${request.op}`);
	}
}

function errorResult(id: unknown, error: unknown): unknown {
	const name = error instanceof Error ? error.name : "Error";
	const message = error instanceof Error ? error.message : String(error);
	return { id, ok: false, error: { name, message } };
}

async function respond(line: string): Promise<void> {
	let id: unknown = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const output = await execute(request);
		process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify(errorResult(id, error))}\n`);
	}
}

async function main(): Promise<void> {
	const input = createInterface({ input: process.stdin });
	for await (const line of input) await respond(line);
}

void main().catch((error: unknown) => {
	process.stderr.write(`${JSON.stringify(errorResult(null, error))}\n`);
	process.exitCode = 1;
});
