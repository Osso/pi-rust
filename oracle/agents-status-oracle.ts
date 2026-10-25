import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Agent = Record<string, unknown>;
type StatusApi = {
    collectAgentRows(agents: Agent[], nowMs: number): Array<{agent: Agent; depth: number}>;
    describeAgentKind(agent: Agent): string;
    renderAgentsStatus(agents: Agent[], width: number, nowMs: number, theme: {fg(color: string, text: string): string}): string[];
};
type Request = {id: unknown; agents: Agent[]; now: number; width: number; colors: Record<string, string>};

async function main(): Promise<void> {
    const file = process.argv[2];
    if (!file) throw new Error("agents-status source path is required");
    const api = await import(pathToFileURL(file).href) as StatusApi;
    for await (const line of createInterface({input: process.stdin})) {
        const input = JSON.parse(line) as Request;
        const theme = {fg: (color: string, text: string) => `${input.colors[color]}${text}\x1b[39m`};
        const rows = api.collectAgentRows(input.agents, input.now).map(row => ({id: row.agent.id, depth: row.depth}));
        const kinds = input.agents.map(api.describeAgentKind);
        const lines = api.renderAgentsStatus(input.agents, input.width, input.now, theme);
        process.stdout.write(`${JSON.stringify({id: input.id, rows, kinds, lines})}\n`);
    }
}
void main().catch((error: unknown) => {
    process.stderr.write(`${String(error)}\n`);
    process.exitCode = 1;
});
