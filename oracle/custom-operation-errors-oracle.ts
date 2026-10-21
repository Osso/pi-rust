import { pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

interface Request {
    tool: "read" | "write" | "edit" | "ls" | "grep" | "find" | "bash";
    stage: string;
    message?: string;
    params: Record<string, unknown>;
    abort?: "before" | "reject" | "pending";
}

function failingOperation<T>(request: Request, stage: string, value: T) {
    return async () => {
        if (request.stage === stage) {
            throw new Error(request.message ?? "SSH failed (7): scripted failure\n");
        }
        return value;
    };
}

async function run(request: Request) {
    const modulePath = join(dirname(process.argv[2]), `${request.tool}.ts`);
    const module = await import(pathToFileURL(modulePath).href);
    const fault = <T>(stage: string, value: T) => failingOperation(request, stage, value);
    const controller = new AbortController();
    if (request.abort === "before") controller.abort();
    const rejectOperation = async (stage: string, value: unknown) => {
        if (request.stage !== stage) return value;
        if (request.abort === "pending") {
            setTimeout(() => controller.abort(), 0);
            return new Promise(() => {});
        }
        if (request.abort === "reject") controller.abort();
        throw new Error(request.message ?? "SSH failed (7): scripted failure\n");
    };
    const operations = {
        access: fault("access", undefined),
        readFile: fault(request.stage === "image" ? "image" : "readFile", Buffer.from("old")),
        detectImageMimeType: fault("mime", request.stage === "image" ? "image/png" : null),
        stat: fault("stat", { size: 100000, isDirectory: () => true }),
        mkdir: fault("mkdir", undefined),
        writeFile: fault("writeFile", undefined),
        exists: () => rejectOperation("exists", request.stage !== "missing"),
        readdir: fault("readdir", []),
        isDirectory: fault("directory", true),
        glob: () => rejectOperation("glob", []),
        exec: fault(request.stage, { exitCode: 0 }),
        ...(request.stage === "range" ? { readRange: fault("range", Buffer.alloc(0)) } : {}),
    };
    const factoryName = `create${request.tool[0].toUpperCase() + request.tool.slice(1)}Tool`;
    const create = module[factoryName];
    try {
        await create("/fixture", { operations }).execute("fault", request.params, controller.signal);
        return { error: null };
    } catch (error) {
        if (error instanceof Error) return { error: error.message };
        throw error;
    }
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
