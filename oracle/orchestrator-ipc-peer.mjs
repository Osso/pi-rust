import { pathToFileURL } from "node:url";
const [root, mode, payload] = process.argv.slice(2);
const load = (name) => import(pathToFileURL(`${root}/packages/orchestrator/src/${name}.ts`).href);
if (mode === "client") {
    const { sendIpcRequest } = await load("ipc/client");
    process.stdout.write(`${JSON.stringify(await sendIpcRequest(JSON.parse(payload)))}\n`);
} else {
    const { startIpcServer } = await load("ipc/server");
    const handler = async (request) => request.type === "rpc_stream"
        ? { type: "rpc_ready", ok: true, instance: { id: "i" } }
        : { type: "list_result", ok: true, instances: [] };
    handler.openRpcStream = (_id, response, event, ui) => ({
        async handleRequest(request) {
            if (request.type === "extension_ui_response") return;
            response({ type: "response", command: request.type, success: true, id: request.id });
        },
        close() {},
    });
    const server = await startIpcServer(handler);
    process.stdout.write("ready\n");
    process.stdin.resume();
    process.stdin.on("end", () => server.close());
}
