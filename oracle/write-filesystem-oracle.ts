import { chmod, mkdir, readFile, readdir, stat, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { root: string; setup?: string; path: string; content: string; probe?: string | null };
type Api = { createWriteToolDefinition(cwd: string): { execute(id: string, input: unknown): Promise<unknown> } };

async function createFilesystemFixture(input: Input) {
    switch (input.setup) {
        case "directory": await mkdir(join(input.root, "folder")); break;
        case "parent-file": await writeFile(join(input.root, "leaf"), "untouched"); break;
        case "overwrite": {
            const path = join(input.root, "plain.txt");
            await writeFile(path, "longer untouched initial");
            await chmod(path, 0o640);
            break;
        }
        case "readonly": {
            const path = join(input.root, "locked.txt");
            await writeFile(path, "untouched");
            await chmod(path, 0o444);
            break;
        }
        case "symlink-parent":
            await mkdir(join(input.root, "target"));
            await symlink("target", join(input.root, "link"));
            break;
        case "symlink-loop": await symlink("loop.txt", join(input.root, "loop.txt")); break;
        case "symlink-file":
            await writeFile(join(input.root, "target.txt"), "untouched");
            await symlink("target.txt", join(input.root, "link.txt"));
            break;
    }
}

async function readProbe(input: Input) {
    if (input.probe === null) return { content: null, mode: null };
    const path = join(input.root, input.probe ?? input.path);
    try { return { content: await readFile(path, "utf8"), mode: (await stat(path)).mode & 0o777 }; }
    catch (error) {
        const code = (error as NodeJS.ErrnoException).code;
        if (["ENOENT", "EISDIR", "ENOTDIR", "ERR_INVALID_ARG_VALUE"].includes(code ?? "")) return { content: null, mode: null };
        throw error;
    }
}

void runOracle(async (module, request) => {
    const api = module as Api;
    const output = [];
    for (const value of requireCases(request)) {
        const input = value as Input;
        await createFilesystemFixture(input);
        let result;
        try {
            result = { ok: true, value: await api.createWriteToolDefinition(input.root).execute("write-fs", { path: input.path, content: input.content }) };
        } catch (error) {
            if (!(error instanceof Error)) throw error;
            result = { ok: false, error: error.message.replaceAll(input.root, "ROOT") };
        }
        output.push({ result, ...await readProbe(input), entries: (await readdir(input.root)).sort() });
    }
    return output;
}).catch((error: unknown) => {
    process.stderr.write(`write-filesystem oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
