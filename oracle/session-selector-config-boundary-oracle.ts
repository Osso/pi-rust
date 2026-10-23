import { stripVTControlCharacters } from "node:util";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (module, request) => {
    const theme = await import(pathToFileURL(join(dirname(process.argv[2]), "theme/theme.ts")).href);
    theme.initTheme("dark", false);
    const api = module as unknown as { InteractiveMode: { prototype: object } };
    const mode = Object.create(api.InteractiveMode.prototype);
    let lookups = 0;
    Object.defineProperty(mode, "sessionManager", { value: {
        getCwd: () => request.cwd,
        getSessionDir: () => `${request.cwd}/sessions`,
        getSessionFile: () => undefined,
        usesDefaultSessionDir: () => { lookups++; throw new Error('File URL host must be "localhost" or empty on linux'); },
    } });
    mode.options = {};
    mode.sortNamedSessionsFirst = (sessions: unknown[]) => sessions;
    type Selector = { currentLoading: boolean; allLoading: boolean; handleInput(input: string): void; render(width: number): string[] };
    let selector!: Selector;
    mode.showSelector = (factory: (done: () => void) => { component: Selector }) => { selector = factory(() => {}).component; };
    mode.ui = { requestRender() {} };
    mode.showSessionSelector();
    const before = lookups;
    for (let i = 0; i < 100 && selector.currentLoading; i++) await new Promise(resolve => setTimeout(resolve, 5));
    selector.handleInput("\t");
    for (let i = 0; i < 100 && selector.allLoading; i++) await new Promise(resolve => setTimeout(resolve, 5));
    const output = stripVTControlCharacters(selector.render(200).join("\n"));
    return { before, lookups, caught: output.includes("Failed to load sessions: File URL host must be") };
}).catch((error: unknown) => { console.error(error); process.exitCode = 1; });
