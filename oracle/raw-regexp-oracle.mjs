import { createInterface } from "node:readline";
const native = (units) => String.fromCharCode(...units);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const text = native(request.units);
		const flags = `${request.flags}dg`;
		const regexp = new RegExp(request.pattern, flags);
		regexp.lastIndex = request.start;
		const match = regexp.exec(text);
		const captures = match ? [match.indices.map((range, index) => range ? [...range, match[index]] : null)] : [];
		const replaced = text.replace(new RegExp(request.pattern, flags), native(request.replacement));
		const callback = text.replace(new RegExp(request.pattern, flags), (value) => `\ud800${value}`);
		process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify([captures, replaced, callback]) })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
	}
}
