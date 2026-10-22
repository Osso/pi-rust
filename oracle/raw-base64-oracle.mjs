import { createInterface } from "node:readline";
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const text = String.fromCharCode(...request.units);
		process.stdout.write(`${JSON.stringify({ ok: true, hex: Buffer.from(text, "base64").toString("hex") })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
	}
}
