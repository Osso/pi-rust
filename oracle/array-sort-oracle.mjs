import { createInterface } from "node:readline";

function compare(mode, left, right, count) {
	if (mode === "date") return left.key - right.key;
	if (mode === "boolean") return left.key > right.key ? 1 : 0;
	if (mode === "cycle") return left.id % 3 === right.id % 3 ? 0 : (left.id % 3 + 1) % 3 === right.id % 3 ? -1 : 1;
	if (mode === "alternating") return count % 5 === 0 ? NaN : count % 2 === 0 ? left.key - right.key : right.key - left.key;
	if (mode === "table") {
		const bucket = (left.id * 17 + right.id * 31) % 7;
		return [NaN, -Infinity, Infinity, -0, -0.25, 0.25, 0][bucket];
	}
	throw new Error(`Unknown comparator ${mode}`);
}
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const values = request.keys.map((key, id) => ({ id, key: Number(key) }));
	const trace = [];
	let count = 0;
	values.sort((left, right) => {
		const result = compare(request.mode, left, right, ++count);
		trace.push([left.id, right.id, String(result)]);
		return result;
	});
	process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, order: values.map(value => value.id), trace })}\n`);
}
