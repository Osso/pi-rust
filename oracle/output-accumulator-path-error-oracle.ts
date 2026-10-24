import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Snapshot = {
	content: string;
	truncation: unknown;
	fullOutputPath?: string;
};
type Api = {
	OutputAccumulator: new (options: {
		maxBytes: number;
		tempFilePrefix: string;
	}) => {
		append(data: Buffer): void;
		finish(): void;
		snapshot(): Snapshot;
		closeTempFile(): Promise<void>;
	};
};

void runOracle(async (module) => {
	const api = module as unknown as Api;
	const output = new api.OutputAccumulator({
		maxBytes: 1,
		tempFilePrefix: "pi-acc\0",
	});
	let thrown: { name: string; message: string; code?: string } | null = null;
	try {
		output.append(Buffer.from("payload"));
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		const native = error as NodeJS.ErrnoException;
		thrown = { name: native.name, message: native.message, code: native.code };
	}
	output.finish();
	const snapshot = output.snapshot();
	await output.closeTempFile();
	return {
		thrown,
		snapshot,
		truncationJson: JSON.stringify(snapshot.truncation),
		close: "resolved",
	};
}).catch((error: unknown) => {
	process.stderr.write(
		`accumulator path-error oracle failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
