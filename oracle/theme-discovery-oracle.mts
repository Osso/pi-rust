import { createInterface } from "node:readline";
import { basename } from "node:path";
import { pathToFileURL } from "node:url";
const api = await import(pathToFileURL(process.argv[2]!).href);
for await (const line of createInterface({ input: process.stdin })) {
 const request = JSON.parse(line);
 api.setRegisteredThemes([new api.Theme({}, {}, "256color", { name: "duplicate", sourcePath: "/registered.json" }), new api.Theme({}, {}, "256color", { name: "registered" }),new api.Theme({}, {}, "256color", { name: "empty-path",sourcePath:"" }),new api.Theme({}, {}, "256color", {name:"file-export",sourcePath:request.exportPath})]);
 const output = api.getAvailableThemesWithPaths().map(({ name, path }) => ({ name, file: path === undefined ? null : basename(path) }));
 const exports = ["registered","empty-path"].map(name => {try {return api.getResolvedThemeColors(name);} catch(error) {return String(error);}});
 console.log(JSON.stringify({ id: request.id, ok: true, output:{themes:output,exports,colorKeys:Object.keys(api.getResolvedThemeColors("file-export"))} }));
}
