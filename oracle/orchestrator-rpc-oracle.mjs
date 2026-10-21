import { pathToFileURL } from "node:url";
const [root, fake] = process.argv.slice(2);
const { RpcProcessInstance } = await import(pathToFileURL(`${root}/packages/orchestrator/src/rpc-process.ts`).href);
// Inject only the child executable; frozen routing/lifetime implementation remains unchanged.
RpcProcessInstance.prototype.getSpawnCommand = () => ({command: process.execPath, args: [fake]});
const rpc = new RpcProcessInstance({cwd: process.cwd()});
const events = [], ui = [];
rpc.onEvent((v) => events.push(v));
rpc.setUiRequestHandler((v) => ui.push(v));
const responses = await Promise.all([rpc.send({type:"get_state",id:"a"}), rpc.send({type:"get_state",id:"b"})]);
await rpc.dispose();
process.stdout.write(`${JSON.stringify({responses,events,ui})}\n`);
