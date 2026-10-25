// Real process/terminal boundary for frozen handleConfigCommand; no module mocks.
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("config dispatch source module required");
const api = await import(pathToFileURL(source).href);
const handled = await api.handleConfigCommand(process.argv.slice(3));
if (handled !== false) throw new Error("handled config must exit from the frozen command");
process.exitCode = 3;
