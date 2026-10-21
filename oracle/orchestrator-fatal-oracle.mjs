import { pathToFileURL } from "node:url";
const [root,fake] = process.argv.slice(2);
delete process.env.PI_RADIUS_API_KEY;
const { RpcProcessInstance } = await import(pathToFileURL(`${root}/packages/orchestrator/src/rpc-process.ts`).href);
RpcProcessInstance.prototype.getSpawnCommand = () => ({command:process.execPath,args:[fake]});
const { serve } = await import(pathToFileURL(`${root}/packages/orchestrator/src/serve.ts`).href);
await serve();
