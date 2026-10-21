import { pathToFileURL } from "node:url";
delete process.env.HOME;
delete process.env.PI_ORCHESTRATOR_DIR;
delete process.env.PI_CONFIG_DIR;
const config = await import(pathToFileURL(`${process.argv[2]}/packages/orchestrator/src/config.ts`).href);
process.stdout.write(`${config.getOrchestratorDir()}\n`);
