import { createInterface } from "node:readline";
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const output = request.values.map((value) => {
		const date = new Date(Number(value));
		return {
			fields: Number.isNaN(date.getTime())
				? null
				: [
						date.getFullYear(),
						date.getMonth(),
						date.getDate(),
						date.getHours(),
						date.getMinutes(),
					],
			full: date.toLocaleString("en-US"),
			timeUs: date.toLocaleTimeString("en-US"),
			time: date.toLocaleTimeString("en-GB", {
				hour: "2-digit",
				minute: "2-digit",
				second: "2-digit",
				hourCycle: "h23",
			}),
		};
	});
	process.stdout.write(`${JSON.stringify(output)}\n`);
}
