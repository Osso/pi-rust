import { createInterface } from "node:readline";
import { Writable } from "node:stream";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
const core = dirname(process.argv[2]);
const load = path => import(pathToFileURL(join(core, path)).href);
const sdk = await load("sdk.ts");
const services = await load("agent-session-services.ts");
const main = await load("../main.ts");
const login = await load("../cli/login-command.ts");
const config = await load("../config.ts");
const control = await load("../cli/control-command.ts");
const debug = await load("../cli/debug-command.ts");
const sessions = await load("../cli/sessions-command.ts");
const startup = await load("../cli/startup-ui.ts");
const settings = await load("settings-manager.ts");
const architect = await load("../architect/main.ts");
const supervisor = await load("../supervisor/main.ts");
const relay = await load("host-relay.ts");
const relayResident = await load("host-relay-resident.ts");
function errorValue(error) {
    const value = { name: error.name, message: error.message };
    for (const key of Object.getOwnPropertyNames(error)) {
        if (!["stack", "name", "message"].includes(key)) value[key] = error[key];
    }
    return { value, constructor: error.constructor.name, text: String(error) };
}
async function capture(action) {
    try { await action(); return { ok: true }; }
    catch (error) { return errorValue(error); }
}
for await (const line of createInterface({ input: process.stdin })) {
    JSON.parse(line);
    for (const name of ["HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", config.ENV_AGENT_DIR, config.ENV_STATE_DIR]) delete process.env[name];
    process.env.PI_TEST_FAIL_PASSWD = "1";
    const dependencies = { error: new Writable({ write(_chunk, _encoding, done) { done(); } }) };
    const output = {
        sdk: await capture(() => sdk.createAgentSession()),
        services: await capture(() => services.createAgentSessionServices({ cwd: "/fixture" })),
        main: await capture(() => main.main(["--version"])),
        loginHelp: await capture(() => login.handleLoginCommand(["login", "--help"], dependencies)),
        login: await capture(() => login.handleLoginCommand(["login", "openai-codex"], dependencies)),
        control: await capture(() => control.handleControlCommand(["control", "path"], {})),
        debug: await capture(() => debug.handleDebugCommand(["debug", "attach", "fixture"], {})),
        sessions: await capture(() => sessions.handleSessionsCommand(["sessions", "truncate-tool-output"], {})),
        startup: await capture(() => startup.createStartupTui(settings.SettingsManager.inMemory())),
        architect: await capture(() => architect.runArchitectService()),
        supervisor: await capture(() => supervisor.runSupervisorService()),
        relay: await capture(() => relay.runRelayServe()),
        relayResident: await capture(() => relayResident.runRelayConnect()),
    };
    process.stdout.write(`${JSON.stringify(output)}\n`);
}
