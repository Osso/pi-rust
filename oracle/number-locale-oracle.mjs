import { createInterface } from "node:readline";
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const view = new DataView(new ArrayBuffer(8));
		const output = request.bits.map((bits) => {
			view.setBigUint64(0, BigInt(`0x${bits}`), false);
			return view.getFloat64(0, false).toLocaleString("en-US");
		});
		process.stdout.write(`${JSON.stringify({ ok: true, output })}\n`);
	} catch (error) {
		process.stdout.write(
			`${JSON.stringify({ ok: false, error: String(error) })}\n`,
		);
	}
}
