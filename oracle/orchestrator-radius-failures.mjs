import { pathToFileURL } from "node:url";
const [root, scenario] = process.argv.slice(2);
process.env.PI_RADIUS_API_KEY = "key";
const NativeDate = Date;
globalThis.Date = class extends NativeDate {
    constructor(...args) { super(...(args.length ? args : ["2026-10-21T00:00:00.000Z"])); }
};
Math.random = () => 0.5;
const schedules = [], errors = [], requests = [], updates = [];
globalThis.setTimeout = (_callback, delay) => { schedules.push(delay); return schedules.length; };
globalThis.clearTimeout = () => {};
console.error = message => errors.push(message);
const registration = (id, interval) => ({ id, heartbeatIntervalMs: interval, expiresInMs: interval * 2 });
const machine = scenario.startsWith("machine");
const sequence = [[200, registration("m1", machine ? 10 : 1000000)]];
if (!machine) sequence.push([200, registration("p1", 10)]);
sequence.push([404, "gone"], [404, "gone"], [404, "gone"]);
if (scenario === "machine_failure") {
    sequence.push([503, "recover"], [200, {}]);
} else if (scenario === "machine_pi_failure") {
    sequence.push([200, registration("m2", 10)], [503, "recover"]);
} else if (scenario === "pi_failure") {
    sequence.push([503, "recover"], [200, {}]);
}
if (!machine) sequence.push([404, "gone"]);
sequence.push([404, "gone"]);
globalThis.fetch = async (url, init) => {
    const body = JSON.parse(init.body);
    for (const key of ["hostname", "platform", "arch", "pid"]) delete body[key];
    if (String(url).includes("/machines/") && String(url).endsWith("/heartbeat")) {
        delete body.cwd;
        delete body.socketPath;
    }
    requests.push({ url: String(url), body });
    const next = sequence.shift();
    if (!next) throw new Error("Unexpected extra HTTP request");
    const [status, data] = next;
    return new Response(typeof data === "string" ? data : JSON.stringify(data), { status });
};
const { RadiusPresence } = await import(pathToFileURL(`${root}/packages/orchestrator/src/radius.ts`).href);
const radius = new RadiusPresence();
let instance = { id: "local", cwd: "/tmp", sessionId: "session" };
let available = true;
radius.setCoordinator({
    getLiveInstance: () => available ? instance : undefined,
    listLiveInstances: () => available ? [instance] : [],
    updateInstance: value => { instance = value; updates.push(value); },
});
await radius.start("desk");
if (!machine) instance = await radius.registerPi(instance);
for (let index = 0; index < 3; index++) {
    if (scenario === "pi_disappeared" && index === 2) available = false;
    if (machine) await radius.heartbeatMachine();
    else await radius.heartbeatPi(instance.id);
}
if (scenario === "machine_failure") await radius.heartbeatMachine();
if (scenario === "pi_failure") await radius.heartbeatPi(instance.id);
if (!machine) await radius.disconnectPi(instance);
await radius.stop();
if (sequence.length) throw new Error(`Unused HTTP responses: ${sequence.length}`);
process.stdout.write(`${JSON.stringify({ requests, updates, schedules, errors })}\n`);
