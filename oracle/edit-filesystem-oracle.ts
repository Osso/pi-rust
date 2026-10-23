import { chmod, mkdir, readFile, readdir, stat, symlink, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = { root: string; path: string; edits: unknown[]; setup?: string; probe?: string | null;
    fixture?: { path: string; bytes: number[]; mode?: number } };
type Api = { createEditToolDefinition(cwd: string): {execute(id: string, input: unknown): Promise<unknown>} };

async function createFixture(input: Input) {
    if (input.fixture) {
        const path = join(input.root, input.fixture.path);
        await mkdir(dirname(path), {recursive:true});
        await writeFile(path, Buffer.from(input.fixture.bytes));
        if (input.fixture.mode !== undefined) await chmod(path, input.fixture.mode);
    }
    switch (input.setup) {
        case "directory": await mkdir(join(input.root, "folder")); break;
        case "parent-file": await writeFile(join(input.root, "leaf"), "untouched"); break;
        case "symlink-file": await symlink("file.txt", join(input.root, "alias.txt")); break;
        case "symlink-loop": await symlink("loop.txt", join(input.root, "loop.txt")); break;
    }
}
async function readProbe(input: Input) {
    if (input.probe === null) return {content:null,mode:null};
    const path = join(input.root, input.probe ?? input.path);
    try { return {content:await readFile(path,"utf8"),mode:(await stat(path)).mode & 0o777}; }
    catch (error) {
        if (["ENOENT","ENOTDIR","EISDIR","EACCES","ERR_INVALID_ARG_VALUE"].includes((error as NodeJS.ErrnoException).code ?? "")) return {content:null,mode:null};
        throw error;
    }
}
void runOracle(async (module, request) => {
    const api = module as Api;
    const output = [];
    for (const value of requireCases(request)) {
        const input = value as Input;
        await createFixture(input);
        let result;
        try { result = {ok:true,value:await api.createEditToolDefinition(input.root).execute("edit-fs",{path:input.path,edits:input.edits})}; }
        catch (error) {
            if (!(error instanceof Error)) throw error;
            result = {ok:false,error:error.message.replaceAll(input.root,"ROOT")};
        }
        output.push({result,...await readProbe(input),entries:(await readdir(input.root)).sort()});
    }
    return output;
}).catch((error: unknown) => {
    process.stderr.write(`edit-filesystem oracle failed: ${String(error)}\n`);
    process.exitCode = 1;
});
