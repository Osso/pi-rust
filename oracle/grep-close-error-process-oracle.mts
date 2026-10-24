import * as fs from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
const source = process.argv[2];
const root = process.env.PI_GREP_CLOSE_ROOT;
if (!source || !root) throw new Error("grep close process requires source and root");
const module = await import(pathToFileURL(source).href);
const deleted = join(root, "source-process-deleted");
await fs.mkdir(deleted);
process.chdir(deleted);
await fs.rmdir(deleted);
const tool = module.createGrepToolDefinition(root, { operations: { isDirectory: () => true, readFile: () => "unused" } });
void tool.execute("grep-close-error", { pattern: "x" }).then(
	() => console.log("TOOL_SETTLED"), () => console.log("TOOL_SETTLED"),
);
