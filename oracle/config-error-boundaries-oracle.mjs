import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const api = await import(pathToFileURL(process.argv[2]).href);

function errorValue(error) {
    const output = { name: error.name, message: error.message };
    for (const key of Object.getOwnPropertyNames(error)) {
        if (key !== "stack" && key !== "name" && key !== "message") output[key] = error[key];
    }
    return { error: output, constructor: error.constructor.name, text: String(error) };
}
for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    delete process.env.HOME;
    if (request.home !== undefined) process.env.HOME = request.home;
    for (const key of ["XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", api.ENV_AGENT_DIR, api.ENV_STATE_DIR]) delete process.env[key];
    if (request.cacheHome !== undefined) process.env.XDG_CACHE_HOME = request.cacheHome;
    process.env.PI_TEST_FAIL_PASSWD = request.fail ? "1" : "0";
    if (request.packageDir === undefined) delete process.env.PI_PACKAGE_DIR;
    else process.env.PI_PACKAGE_DIR = request.packageDir;
    const output = {};
    for (const name of request.getters) {
        try { output[name] = { value: api[name]() }; }
        catch (error) { output[name] = errorValue(error); }
    }
    process.stdout.write(`${JSON.stringify(output)}\n`);
}
