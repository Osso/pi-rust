import { createInterface } from 'node:readline';
import { createRequire } from 'node:module';
const ignore = createRequire(import.meta.url)(process.argv[2]);
const string = units => String.fromCharCode(...units);
const units = text => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
for await (const line of createInterface({ input: process.stdin })) {
  const { rules, paths } = JSON.parse(line);
  const matcher = ignore();
  for (const rule of rules) matcher.add(string(rule));
  const results = paths.map(path => {
    try { return { ignored: matcher.ignores(string(path)) }; }
    catch (error) { return { name: units(error.name), message: units(error.message) }; }
  });
  console.log(JSON.stringify(results));
}
