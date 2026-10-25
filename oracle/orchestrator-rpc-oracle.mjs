import { pathToFileURL } from "node:url";
const [root, fake] = process.argv.slice(2);
const { RpcProcessInstance } = await import(pathToFileURL(`${root}/packages/orchestrator/src/rpc-process.ts`).href);
// Inject only the child executable; frozen routing/lifetime implementation remains unchanged.
RpcProcessInstance.prototype.getSpawnCommand = () => ({command: process.execPath, args: [fake]});
const rpc = new RpcProcessInstance({cwd: process.cwd()});
if (process.argv[4] === "duplicate") {
    const original = rpc.send({type:"hang",id:"duplicate"});
    const response = await rpc.send({type:"get_state",id:"duplicate"});
    const staysPending = () => Promise.race([original.then(() => false, () => false),new Promise(resolve => setTimeout(() => resolve(true),30))]);
    const before = await staysPending();
    await rpc.dispose();
    const after = await staysPending();
    process.stdout.write(`${JSON.stringify({before,after,command:response.command})}\n`);
    process.exit(0);
}
if (["unterminated","invalid_utf8","stderr_utf8"].includes(process.argv[4])) {
    let output;
    try { output = { response: await rpc.send({type:process.argv[4],id:"frame"}) }; }
    catch(error) { output = { error:error.message }; }
    await rpc.dispose();
    process.stdout.write(`${JSON.stringify(output)}\n`);
    process.exit(0);
}
if (process.argv[4] === "listener_identity") {
    let events = 0, exits = 0;
    const event = () => events++, exit = () => exits++;
    const unsubscribe = rpc.onEvent(event);rpc.onEvent(event);rpc.onExit(exit);rpc.onExit(exit);
    await rpc.send({type:"get_state",id:"one"});const first = events;
    unsubscribe();await rpc.send({type:"get_state",id:"two"});await rpc.dispose();
    process.stdout.write(`${JSON.stringify({first,events,exits})}\n`);process.exit(0);
}
if (process.argv[4] === "pending_order") {
    const ids = Array.from({ length: 8 }, (_, index) => `pending_${index}`);
    const rejected = [];
    let received = 0;
    const ready = new Promise(resolve => rpc.onEvent(() => {
        if (++received === ids.length) resolve();
    }));
    const pending = ids.map(id => rpc.send({ type: "hold", id }).catch(() => rejected.push(id)));
    await ready;
    await rpc.dispose();
    await Promise.all(pending);
    process.stdout.write(`${JSON.stringify(rejected)}\n`);
    process.exit(0);
}
if (process.argv[4] === "listener_mutation") {
    const events = [], exits = [];
    const lateEvent = () => events.push("late");
    const lateExit = () => exits.push("late");
    let removeEvent, removeExit;
    rpc.onEvent(() => {
        events.push("first");
        removeEvent();
        rpc.onEvent(lateEvent);
    });
    removeEvent = rpc.onEvent(() => events.push("removed"));
    rpc.onExit(() => {
        exits.push("first");
        removeExit();
        rpc.onExit(lateExit);
    });
    removeExit = rpc.onExit(() => exits.push("removed"));
    await rpc.send({ type: "get_state", id: "one" });
    await rpc.dispose();
    process.stdout.write(`${JSON.stringify({events,exits})}\n`);
    process.exit(0);
}
const events = [], ui = [];
rpc.onEvent((v) => events.push(v));
rpc.setUiRequestHandler((v) => ui.push(v));
const responses = await Promise.all([rpc.send({type:"get_state",id:"a"}), rpc.send({type:"get_state",id:"b"})]);
await rpc.dispose();
process.stdout.write(`${JSON.stringify({responses,events,ui})}\n`);
