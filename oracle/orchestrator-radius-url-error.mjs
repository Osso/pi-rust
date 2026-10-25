import { pathToFileURL } from "node:url";
const [root, mode] = process.argv.slice(2);
delete process.env.PI_RADIUS_URL;
delete process.env.PI_RADIUS_ORCHESTRATOR_URL;
process.env.PI_RADIUS_API_KEY = "key";
process.env[mode === "base" ? "PI_RADIUS_URL" : "PI_RADIUS_ORCHESTRATOR_URL"] = "not a URL";
globalThis.fetch = async () => { throw new Error("Unexpected HTTP request"); };
const { RadiusPresence } = await import(pathToFileURL(`${root}/packages/orchestrator/src/radius.ts`).href);
try {
    await new RadiusPresence().start();
    throw new Error("Expected invalid URL failure");
} catch (error) {
    process.stdout.write(`${JSON.stringify(error.message)}\n`);
}
