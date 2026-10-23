import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";

const source = process.argv[2]!;
const tools = dirname(source);
const renderUtils = await import(pathToFileURL(source).href);
const read = await import(pathToFileURL(resolve(tools, "read.ts")).href);
const themes = await import(pathToFileURL(resolve(tools, "../../modes/interactive/theme/theme.ts")).href);
const execution = await import(pathToFileURL(resolve(tools, "../../modes/interactive/components/tool-execution.ts")).href);
const tui = await import(pathToFileURL(resolve(tools, "../../../../tui/src/index.ts")).href);
themes.initTheme("dark");
tui.setCapabilities({ images: null, trueColor: true, hyperlinks: false });

function capture(callback: () => unknown): unknown {
    try { return { value: callback() }; }
    catch (error) { return { error: (error as Error).message }; }
}

for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    delete process.env.HOME;
    process.env.PI_TEST_FAIL_PASSWD = request.fail ? "1" : "0";
    const args = { path: request.path };
    const definition = read.createReadToolDefinition("/repo");
    const context = { args, cwd: "/repo", expanded: true, state: {}, invalidate() {} };
    const component = new execution.ToolExecutionComponent("read", "home-read", args, {}, definition, { requestRender() {} }, "/repo");
    const output = {
        shorten: capture(() => renderUtils.shortenPath(request.path)),
        call: capture(() => definition.renderCall(args, themes.theme, context).render(80)),
        component: component.render(80),
    };
    process.stdout.write(`${JSON.stringify(output)}\n`);
}
