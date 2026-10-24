import { pathToFileURL } from "node:url";

function emit(event: string, error?: unknown): void {
	const system =
		error instanceof Error ? (error as NodeJS.ErrnoException) : undefined;
	const details = system
		? {
				name: system.name,
				message: system.message,
				code: system.code,
				errno: system.errno,
				syscall: system.syscall,
				path: system.path,
			}
		: undefined;
	process.stdout.write(`${JSON.stringify({ event, error: details })}\n`);
}

process.once("uncaughtException", (error) => {
	emit("uncaught", error);
	process.exitCode = 1;
});

const source = (await import(pathToFileURL(process.argv[2]).href)) as {
	OutputAccumulator: new (options: {
		maxBytes: number;
		tempFilePrefix: string;
	}) => {
		append(data: Buffer): void;
		closeTempFile(): Promise<void>;
	};
};
const fault = process.env.PI_ACC_FAULT ?? "open";
const accumulator = new source.OutputAccumulator({
	maxBytes: fault === "replay-write" ? 4 : 1,
	tempFilePrefix: fault === "open" ? "blocked/pi-acc" : "pi-acc",
});
emit("append-start");
if (fault === "replay-write") accumulator.append(Buffer.from("ab"));
accumulator.append(Buffer.from("payload"));
emit("append-returned");
if (process.env.PI_ACC_CLOSE === "1") {
	emit("close-start");
	try {
		await accumulator.closeTempFile();
		emit("close-resolved");
		if (fault === "close") {
			await accumulator.closeTempFile();
			emit("second-close-resolved");
		}
	} catch (error) {
		emit("close-rejected", error);
	}
}
