import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const { ProcessTerminal } = await import(pathToFileURL(process.argv[2]).href);
	const cycles = Number(process.env.Q1_STREAM_CYCLES ?? "1");
	for (let cycle = 0; cycle < cycles; cycle++) {
		const terminal = new ProcessTerminal();
		const inputs: string[] = [];
		const sizes: number[][] = [];
		let finish: () => void = () => {};
		const done = new Promise<void>((resolve) => { finish = resolve; });
		terminal.start((data: string) => {
			inputs.push(data);
			if (data === "q") finish();
		}, () => sizes.push([terminal.columns, terminal.rows]));
		terminal.setProgress(true);
		await new Promise((resolve) => setTimeout(resolve, 25));
		process.stderr.write("READY\n");
		await done;
		const kitty = terminal.kittyProtocolActive;
		const modify = terminal.modifyOtherKeysActive;
		const drainStart = Date.now();
		await terminal.drainInput(100, 30);
		const waitedForLateData = Date.now() - drainStart >= 40;
		terminal.stop();
		process.stderr.write(`RESULT ${JSON.stringify({ inputs, sizes, kitty, modify, waitedForLateData })}\n`);
		await new Promise((resolve) => setTimeout(resolve, 40));
}
	process.exit(0);

}
void main().catch((error: unknown) => { console.error(error); process.exit(1); });
