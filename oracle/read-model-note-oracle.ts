import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type ReadApi = { createReadToolDefinition(cwd: string, options: {
    autoResizeImages: boolean;
    operations: { access(): Promise<void>; detectImageMimeType(): Promise<string>; readFile(): Promise<Buffer> };
}): { execute(id: string, args: { path: string }, signal: undefined, update: undefined,
    context: { model: { input: string[] } } | undefined): Promise<unknown> } };

void runOracle(async (module, request) => {
    const api = module as unknown as ReadApi;
    const results = [];
    for (const value of requireCases(request)) {
        const input = value as { input?: string[]; mime: string };
        const definition = api.createReadToolDefinition(String(request.cwd), {
            autoResizeImages: false,
            operations: {
                access: async () => {},
                detectImageMimeType: async () => input.mime,
                readFile: async () => Buffer.from("not-an-image"),
            },
        });
        const context = input.input ? { model: { input: input.input } } : undefined;
        results.push(await definition.execute("image-note", { path: "image" }, undefined, undefined, context));
    }
    return results;
});
