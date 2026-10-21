import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

interface Call { tool: string; params: Record<string, unknown> }
interface Request { cwd: string; flag?: string; calls: Call[] }
interface Tool {
    name: string;
    label: string;
    description: string;
    parameters: unknown;
    execute(id: string, params: Record<string, unknown>): Promise<unknown>;
}
interface Operations {
    exec(command: string, cwd: string, options: {
        timeout?: number;
        signal: AbortSignal;
        onData(bytes: Buffer): void;
    }): Promise<{ exitCode: number | null }>;
}
interface Ui {
    theme: { fg(color: string, text: string): string };
    setStatus(key: string, text: string): void;
    notify(text: string, level: string): void;
}
interface Hooks {
    session_start(event: { type: string }, context: { ui: Ui }): Promise<void>;
    before_agent_start(event: { systemPrompt: string }): Promise<{ systemPrompt?: string } | undefined>;
    user_bash(event: { type: string; command: string }): { operations: Operations } | undefined;
}

const example = import(pathToFileURL(process.argv[2]).href);
const themeModule = import(pathToFileURL(join(dirname(process.argv[2]), "../../src/modes/interactive/theme/theme.ts")).href);

function errorMessage(error: unknown): string {
    if (error instanceof Error) return error.message;
    throw error;
}

async function executeTools(tools: Tool[], calls: Call[]): Promise<unknown[]> {
    const results: unknown[] = [];
    for (const call of calls) {
        const tool = tools.find(tool => tool.name === call.tool);
        if (!tool) throw new Error(`Missing fixture tool: ${call.tool}`);
        try { results.push({ result: await tool.execute("call", call.params) }); }
        catch (error) { results.push({ error: errorMessage(error) }); }
    }
    return results;
}

function bashCommand(mode: string): string {
    switch (mode) {
        case "overflow":
        case "huge":
        case "timeout": return "exec sleep 30";
        case "abort": return "exec python3 -c 'import signal,sys,time; signal.signal(signal.SIGTERM,lambda *_:(print(\"terminated\",flush=True),sys.exit(0))); print(\"ready\",flush=True); time.sleep(30)'";
        case "exit": return "printf code; exit 7";
        default: return "printf 'user output'";
    }
}

async function runUserOperations(ops: Operations | undefined, cwd: string): Promise<unknown[]> {
    if (!ops) return [];
    const results: unknown[] = [];
    for (const mode of ["success", "exit", "timeout", "overflow", "huge", "abort", "preabort"]) {
        let output = "";
        const controller = new AbortController();
        if (mode === "preabort") controller.abort();
        const timeouts: Record<string, number> = { timeout: 1, overflow: 2147483.648, huge: 1e100 };
        const options = {
            timeout: timeouts[mode],
            signal: controller.signal,
            onData(bytes: Buffer) {
                output += bytes.toString();
                if (mode === "abort") controller.abort();
            },
        };
        try {
            const result = await ops.exec(bashCommand(mode), cwd, options);
            results.push({ mode, output, result });
        } catch (error) { results.push({ mode, output, error: errorMessage(error) }); }
    }
    return results;
}

async function registerExample(request: Request) {
    process.chdir(request.cwd);
    const tools: Tool[] = [];
    const hooks = {} as Hooks;
    const trace: unknown[] = [];
    const flag = { value: undefined as string | undefined };
    const { Theme } = await themeModule;
    const theme = new Theme(
        { accent: "#abcdef", text: "#abcdef", thinkingXhigh: "#abcdef" },
        { selectedBg: "#abcdef" }, "truecolor",
    );
    const ui: Ui = {
        theme,
        setStatus: (key, text) => { trace.push(["status", key, text]); },
        notify: (text, level) => { trace.push(["notify", text, level]); },
    };
    (await example).default({
        registerFlag: (name: string, options: unknown) => { trace.push(["flag", name, options]); },
        getFlag: () => flag.value,
        registerTool: (tool: Tool) => { tools.push(tool); },
        on: (name: keyof Hooks, handler: Hooks[keyof Hooks]) => { Object.assign(hooks, { [name]: handler }); },
    });
    return { tools, hooks, trace, flag, ui };
}

async function run(request: Request) {
    const { tools, hooks, trace, flag, ui } = await registerExample(request);
    const metadata = tools.map(({ name, label, description, parameters }) => ({ name, label, description, parameters }));
    // Flags become available only after factory execution.
    const before = await tools[0].execute("before", { path: "read.txt" });
    flag.value = request.flag;
    let startError: string | null = null;
    try { await hooks.session_start({ type: "session_start" }, { ui }); }
    catch (error) { startError = errorMessage(error); }
    const prompt = `Current working directory: ${request.cwd}\nCurrent working directory: ${request.cwd}`;
    const changed = await hooks.before_agent_start({ systemPrompt: prompt });
    const results = await executeTools(tools, request.calls);
    const userOps = hooks.user_bash({ type: "user_bash", command: "ignored" });
    const user = await runUserOperations(userOps?.operations, request.cwd);
    return { metadata, before, trace, startError, prompt: changed?.systemPrompt ?? null, results, user };
}

async function main() {
    let pending = "";
    process.stdin.setEncoding("utf8");
    for await (const chunk of process.stdin) {
        pending += chunk;
        let end: number;
        while ((end = pending.indexOf("\n")) !== -1) {
            const line = pending.slice(0, end);
            pending = pending.slice(end + 1);
            console.log(JSON.stringify(await run(JSON.parse(line) as Request)));
        }
    }
}
main().catch(error => { console.error(error); process.exit(1); });
