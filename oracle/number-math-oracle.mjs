import { createInterface } from "node:readline";
const bits = (value) => {
	const bytes = new ArrayBuffer(8);
	const view = new DataView(bytes);
	view.setFloat64(0, value);
	return view.getBigUint64(0).toString(16).padStart(16, "0");
};
for await (const line of createInterface({ input: process.stdin })) {
	const input = JSON.parse(line);
	const number = (hex) => {
		const bytes = new ArrayBuffer(8);
		const view = new DataView(bytes);
		view.setBigUint64(0, BigInt(`0x${hex}`));
		return view.getFloat64(0);
	};
	const left = number(input.left);
	const right = number(input.right);
	process.stdout.write(
		`${JSON.stringify({ min: bits(Math.min(left, right)), max: bits(Math.max(left, right)) })}\n`,
	);
}
