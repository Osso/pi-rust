import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

function errorValue(error) {
    const value = { name: error.name, message: error.message };
    for (const key of Object.getOwnPropertyNames(error)) {
        if (!["stack", "name", "message"].includes(key)) value[key] = error[key];
    }
    return { error: value, constructor: error.constructor.name, text: String(error) };
}

for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    try {
        const api = await import(pathToFileURL(process.argv[2]).href);
        for (const key of [api.ENV_AGENT_DIR, api.ENV_SESSION_DIR, api.ENV_STATE_DIR]) delete process.env[key];
        for (const key of ["XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"]) delete process.env[key];
        if (request.cacheHome !== undefined) process.env.XDG_CACHE_HOME = request.cacheHome;
        const roots = {};
        for (const name of ["getUserConfigRoot", "getUserStateRoot", "getUserCacheRoot", "getLegacyAgentDir", "getAgentDir", "getAuthPath", "getDebugLogPath"]) roots[name] = api[name]();
        process.env[api.ENV_AGENT_DIR] = "/environment-agent";
        const override = api.getAgentDir();
        delete process.env[api.ENV_AGENT_DIR];
        const args = await import(pathToFileURL(join(dirname(process.argv[2]), "cli/args.ts")).href);
        const log = console.log;
        let help = "";
        console.log = (...values) => { help += `${values.map(String).join(" ")}\n`; };
        try { args.printHelp([]); } finally { console.log = log; }
        const encodedName = value => Buffer.from(value).toString("utf8");
        const output = {
            packageName: api.PACKAGE_NAME, version: api.VERSION, appName: api.APP_NAME,
            appTitle: api.APP_TITLE, configDirName: api.CONFIG_DIR_NAME,
            // Native env names are String keys, not human display strings.
            envNames: [api.ENV_AGENT_DIR, api.ENV_SESSION_DIR, api.ENV_STATE_DIR].map(encodedName),
            roots, override, helpLabels: Buffer.from(help).toString("utf8").split("\n").filter(line => line.includes("- Config directory (default:") || line.includes("- Session storage directory (overridden")),
        };
        process.stdout.write(`${JSON.stringify(output)}\n`);
    } catch (error) {
        process.stdout.write(`${JSON.stringify(errorValue(error))}\n`);
    }
}
