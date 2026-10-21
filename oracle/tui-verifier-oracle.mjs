import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const { Container, TUI } = await import(pathToFileURL(process.argv[2]).href);
const { wrapTextWithAnsi } = await import(pathToFileURL(process.argv[2].replace(/tui\.ts$/, "utils.ts")).href);

function terminal(columns = 80, rows = 24) {
    return {
        columns, rows, writes: "",
        write(data) { this.writes += data; },
        hideCursor() { this.write("\x1b[?25l"); },
        showCursor() { this.write("\x1b[?25h"); },
        start() { this.write("\x1b[?2004h"); },
        stop() { this.write("\x1b[?2004l"); },
    };
}
const text = (value) => ({ render: () => [value], invalidate() {} });

function duplicateChildren() {
    return [new Container(), new TUI(terminal())].map((container) => {
        const child = text("x");
        container.addChild(child);
        container.addChild(text("middle"));
        container.addChild(child);
        container.removeChild(child);
        return container.children.flatMap((entry) => entry.render(80));
    });
}

function listeners(color, live) {
    const tui = new TUI(terminal());
    const add = (callback) => color ? tui.onTerminalColorSchemeChange(callback) : tui.addInputListener(callback);
    const dispatch = () => tui.handleInput(color ? "\x1b[?997;1n" : "a");
    const calls = [];
    if (live) {
        const added = () => { calls.push("added"); };
        let remove;
        add(() => { calls.push("first"); remove(); add(added); });
        remove = add(() => { calls.push("removed"); });
        dispatch();
    } else {
        let count = 0;
        const callback = () => { count++; };
        const first = add(callback);
        const second = add(callback);
        dispatch(); calls.push(count);
        second(); dispatch(); calls.push(count);
        first(); add(callback); dispatch(); calls.push(count);
    }
    return calls;
}

function staleUnsubscribe(color) {
    const tui = new TUI(terminal());
    const add = (callback) => color ? tui.onTerminalColorSchemeChange(callback) : tui.addInputListener(callback);
    let count = 0;
    const callback = () => { count++; };
    const first = add(callback);
    const second = add(callback);
    second();
    add(callback);
    first();
    tui.handleInput(color ? "\x1b[?997;1n" : "a");
    return count;
}

function termux() {
    return [false, true].map((late) => {
        delete process.env.TERMUX_VERSION;
        if (!late) process.env.TERMUX_VERSION = "";
        const term = terminal(8, 10);
        const tui = new TUI(term);
        tui.addChild(text("a"));
        tui.start(); tui.doRender();
        if (late) process.env.TERMUX_VERSION = "1";
        term.rows = 9;
        tui.doRender();
        const redraws = tui.fullRedraws;
        tui.stop();
        return redraws;
    });
}

function crash() {
    const term = terminal(4, 3);
    const tui = new TUI(term);
    let value = "ok";
    tui.addChild({ render: () => [value], invalidate() {} });
    tui.start(); tui.doRender();
    term.writes = "";
    value = "long text";
    let message;
    try { tui.doRender(); } catch (error) { message = error.message.split("\n")[0]; }
    return [message, term.writes];
}

async function region() {
    const tui = new TUI(terminal());
    const handle = tui.createRenderRegion(text("region"));
    let renders = 0;
    tui.addChild({ render() { renders++; return ["root"]; }, invalidate() {} });
    tui.start();
    await new Promise((resolve) => setTimeout(resolve, 100));
    const before = renders;
    const tried = handle.tryRender();
    await new Promise((resolve) => setTimeout(resolve, 100));
    const scheduled = renders > before;
    const requested = handle.requestRender();
    await new Promise((resolve) => setTimeout(resolve, 100));
    const rootRendered = renders > before;
    tui.stop();
    return [tried, scheduled, requested, rootRendered];
}

async function run(scenario) {
    switch (scenario) {
        case "children": return duplicateChildren();
        case "input": return listeners(false, false);
        case "color": return listeners(true, false);
        case "stale-input": return staleUnsubscribe(false);
        case "stale-color": return staleUnsubscribe(true);
        case "live-input": return listeners(false, true);
        case "live-color": return listeners(true, true);
        case "termux": return termux();
        case "crash": return crash();
        case "region": return region();
        case "sgr": return wrapTextWithAnsi("\x1b[x\x1b[31mabcdef", 3);
        default: throw new Error(`Unknown scenario: ${scenario}`);
    }
}

for await (const line of createInterface({ input: process.stdin })) {
    const { scenario } = JSON.parse(line);
    console.log(JSON.stringify(await run(scenario)));
}
