import { pathToFileURL } from "node:url";
import { readFileSync } from "node:fs";
const root = process.argv[2];
const input = JSON.parse(readFileSync(0, "utf8"));
const load = (name) => import(pathToFileURL(`${root}/packages/orchestrator/src/${name}.ts`).href);
let output;
switch (input.op) {
    case "foundation": {
        const config = await load("config");
        const protocol = await load("ipc/protocol");
        const storage = await load("storage");
        storage.saveMachine(input.machine);
        storage.saveInstances(input.instances);
        storage.upsertInstance(input.replacement);
        storage.removeInstance(input.removeId);
        output = {
            dir: config.getOrchestratorDir(),
            version: config.VERSION,
            encoded: input.messages.map(protocol.encodeMessage),
            parsed: input.messages.map((v) => protocol.parseRequestLine(JSON.stringify(v))),
            machine: storage.loadMachine(), instances: storage.loadInstances(),
            machineBytes: readFileSync(config.getMachinePath(), "utf8"),
            instanceBytes: readFileSync(config.getInstancesPath(), "utf8"),
        };
        break;
    }
    case "read_storage": {
        const storage = await load("storage");
        output = { machine: storage.loadMachine(), instances: storage.loadInstances() };
        break;
    }
    default: throw new Error(`Unsupported oracle operation: ${input.op}`);
}
process.stdout.write(`${JSON.stringify(output)}\n`);
