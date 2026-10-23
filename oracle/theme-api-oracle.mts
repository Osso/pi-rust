import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const api = await import(pathToFileURL(process.argv[2]!).href);
function colorValue(value: unknown): unknown {
 if (typeof value === "string" && ["NaN", "Infinity", "-Infinity"].includes(value)) return Number(value);
 return value;
}
for await (const line of createInterface({ input: process.stdin })) {
 const request = JSON.parse(line);
 const capture = (fn: () => unknown) => {
  try { return { ok: true, value: fn() ?? null }; }
  catch (error) { return { ok: false, error: String(error) }; }
 };
 const direct = (value: unknown, name?: string) => new api.Theme(
  Object.fromEntries(request.colors.map((key: string) => [key, colorValue(value)])),
  Object.fromEntries(request.backgrounds.map((key: string) => [key, colorValue(value)])),
  request.mode, { name },
 );
 const output: unknown[] = [];
 for (const value of request.values) output.push(capture(() => {
  const t = direct(value);
  return { fg: t.fg("accent", "a\ud800"), bg: t.bg("selectedBg", "b\udc00"), ansi: [t.getFgAnsi("accent"), t.getBgAnsi("selectedBg")] };
 }));
 api.setRegisteredThemes([direct(1, "old")]);
 output.push(capture(() => api.setRegisteredThemes([direct(2, "first"), direct(3, "bad/name"), direct(4, "last")])));
 output.push(["old", "first", "last"].map(name => api.getThemeByName(name)?.getFgAnsi("accent") ?? null));
 api.onThemeChange(() => { throw new TypeError("callback\ud800"); });
 output.push(capture(() => api.setTheme("first")));
 output.push({ accent: api.theme.getFgAnsi("accent"), name: api.theme.name });
 api.onThemeChange(() => {});
 api.setThemeInstance(new api.Theme({ mdCodeBlock: 7 }, {}, "256color"));
 output.push(capture(() => api.highlightCode("const x = 1;", "javascript")));
 output.push(capture(() => api.getMarkdownTheme().highlightCode("const x = 1;", "javascript")));
 api.setRegisteredThemes([["", 9], ["eq", 1], ["eq", 2], ["Z", 3], ["é", 4], ["é", 5]].map(([name, index]) => direct(index, name as string)));
 output.push({ names: api.getAvailableThemes(), duplicate: api.getThemeByName("eq").getFgAnsi("accent") });
 console.log(JSON.stringify({ id: request.id, ok: true, output }));
}
