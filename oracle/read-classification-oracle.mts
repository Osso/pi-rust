import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { resolve, dirname } from "node:path";
import { mkdtempSync, rmdirSync } from "node:fs";
import { tmpdir } from "node:os";

const source = process.argv[2]!;
const read = await import(pathToFileURL(source).href);
const themes = await import(pathToFileURL(resolve(dirname(source), "../../modes/interactive/theme/theme.ts")).href);
const tui = await import(pathToFileURL(resolve(dirname(source), "../../../../tui/src/index.ts")).href);
themes.initTheme("dark");
tui.setCapabilities({ images: null, trueColor: true, hyperlinks: false });
const originalCwd = process.cwd();
const originalPackageDir = process.env.PI_PACKAGE_DIR;
for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    try {
        if (request.packageDir) process.env.PI_PACKAGE_DIR = request.packageDir;
        tui.setCapabilities({ images: null, trueColor: true, hyperlinks: request.hyperlinks ?? false });
        if (request.deletedCwd) {
            const cwd = mkdtempSync(resolve(tmpdir(), "pi-read-deleted-"));
            process.chdir(cwd);
            rmdirSync(cwd);
        }
        const args = { path: request.path };
        const context = { args, cwd: request.cwd, expanded: request.expanded ?? false, state: {}, invalidate() {} };
        const definition = read.createReadToolDefinition(request.cwd);
        const component = definition.renderCall(args, themes.theme, context);
        process.stdout.write(`${JSON.stringify({value:component.render(100)})}\n`);
    } catch (error) {
        process.stdout.write(`${JSON.stringify({error:(error as Error).message})}\n`);
    } finally {
        process.chdir(originalCwd);
        if (originalPackageDir === undefined) delete process.env.PI_PACKAGE_DIR;
        else process.env.PI_PACKAGE_DIR = originalPackageDir;
    }
}
