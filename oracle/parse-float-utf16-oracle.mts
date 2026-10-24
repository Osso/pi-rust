import { createInterface } from "node:readline";

for await (const line of createInterface({ input: process.stdin })) {
	const { units } = JSON.parse(line) as { units: number[] };
	const value = Number.parseFloat(String.fromCharCode(...units));
	const bytes = Buffer.alloc(8);
	bytes.writeDoubleBE(value);
	process.stdout.write(
		`${JSON.stringify({ bits: Number.isNaN(value) ? "NaN" : bytes.toString("hex") })}\n`,
	);
}
