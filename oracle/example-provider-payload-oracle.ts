import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
const packages = resolve(dirname(source), "../../..");
const load = (relative: string) => import(pathToFileURL(join(packages, relative)).href);

async function run(request: any) {
    const [{ default: register }, loader, runnerModule, sessions, auth, utils, config, events] = await Promise.all([
        import(pathToFileURL(source).href),
        load("coding-agent/src/core/extensions/loader.ts"),
        load("coding-agent/src/core/extensions/runner.ts"),
        load("coding-agent/src/core/session-manager.ts"),
        load("coding-agent/src/core/auth-storage.ts"),
        load("coding-agent/test/model-runtime-test-utils.ts"),
        load("coding-agent/src/config.ts"),
        load("coding-agent/src/core/event-bus.ts"),
    ]);
    const cwd = mkdtempSync(join(tmpdir(), "provider-payload-oracle-"));
    try {
        const logFile = join(cwd, config.CONFIG_DIR_NAME, "provider-payload.log");
        if (request.directory) mkdirSync(dirname(logFile));
        if (request.initial !== null) writeFileSync(logFile, request.initial);
        const runtime = loader.createExtensionRuntime();
        const extension = await loader.loadExtensionFromFactory(register, cwd, events.createEventBus(), runtime);
        const registry = await utils.createInMemoryModelRegistry(auth.AuthStorage.inMemory());
        const runner = new runnerModule.ExtensionRunner([extension], runtime, cwd, sessions.SessionManager.inMemory(), registry);
        const errors: string[] = [];
        runner.onError((error: any) => errors.push(error.event));
        const payloads: any[] = [];
        for (const event of request.events) {
            if (event.type === "before_provider_request") payloads.push(await runner.emitBeforeProviderRequest(event.payload));
            else await runner.emit(event);
        }
        let content = null;
        try { content = readFileSync(logFile, "utf8"); }
        catch (error: any) { if (error.code !== "ENOENT") throw error; }
        return { content, errors, payloads };
    } finally { rmSync(cwd, { recursive: true, force: true }); }
}

async function main() {
    process.stdin.setEncoding("utf8");
    let pending = "";
    for await (const chunk of process.stdin) {
        pending += chunk;
        let end: number;
        while ((end = pending.indexOf("\n")) !== -1) {
            const line = pending.slice(0, end);
            pending = pending.slice(end + 1);
            console.log(JSON.stringify(await run(JSON.parse(line))));
        }
    }
    if (pending.length) throw new Error("Unterminated oracle request");
}
main().catch((error) => { console.error(error); process.exit(1); });
