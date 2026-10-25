import { pathToFileURL } from "node:url";
const root = process.argv[2];
const requests = [];
process.env.PI_RADIUS_API_KEY = "key";
const NativeDate = Date;
globalThis.Date = class extends NativeDate { constructor(...args) { super(...(args.length ? args : ["2026-10-21T00:00:00.000Z"])); } };
let timerId = 0;
globalThis.setTimeout = () => ++timerId;
globalThis.clearTimeout = () => {};
globalThis.fetch = async (url, init) => {
    requests.push({ url: String(url), body: JSON.parse(init.body), headers: init.headers });
    return new Response(JSON.stringify({ id: String(url).endsWith("pis/register") ? "pi-id" : "machine-id", heartbeatIntervalMs:1000000, expiresInMs:2000000 }));
};
const {RadiusPresence} = await import(pathToFileURL(`${root}/packages/orchestrator/src/radius.ts`).href);
const radius = new RadiusPresence();
const machine = await radius.start("desk");
const instance = await radius.registerPi({id:"local",cwd:"/tmp",label:"work",sessionId:"session"});
await radius.disconnectPi(instance);
await radius.stop();
for (const request of requests) {
    delete request.body.hostname;
    delete request.body.platform;
    delete request.body.arch;
    delete request.body.pid;
}
process.stdout.write(`${JSON.stringify({machine,instance,requests})}\n`);
