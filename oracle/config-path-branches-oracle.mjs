import { createInterface } from "node:readline";
import { rmdirSync } from "node:fs";
import { pathToFileURL } from "node:url";

const api = await import(pathToFileURL(process.argv[2]).href);
for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    const cwd = process.cwd();
    const packageDir = process.env.PI_PACKAGE_DIR;
    const descriptor = Object.getOwnPropertyDescriptor(process, "execPath");
    const argv = process.argv.slice();
    try {
        process.env.PI_PACKAGE_DIR = request.packageDir;
        Object.defineProperty(process, "execPath", { value: "/fixture/npm/node", configurable: true });
        process.argv = request.entrypoint ? [process.argv[0], request.entrypoint] : [process.argv[0]];
        if (request.cwd) process.chdir(request.cwd);
        if (request.deleteCwd) rmdirSync(request.cwd);
        const value = request.mode === "instruction"
            ? api.getUpdateInstruction("package")
            : (api.getSelfUpdateCommand("package", [request.tool])?.display ?? null);
        process.stdout.write(`${JSON.stringify({ value })}\n`);
    } catch (error) {
        process.stdout.write(`${JSON.stringify({ error: { name: error.name, message: error.message } })}\n`);
    } finally {
        process.chdir(cwd);
        process.argv = argv;
        Object.defineProperty(process, "execPath", descriptor);
        if (packageDir === undefined) delete process.env.PI_PACKAGE_DIR;
        else process.env.PI_PACKAGE_DIR = packageDir;
    }
}
