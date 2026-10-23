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
    };
    process.stdout.write(`${JSON.stringify(output)}\n`);
}
