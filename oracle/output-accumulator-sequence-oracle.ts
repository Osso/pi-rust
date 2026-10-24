import { readFile, unlink } from "node:fs/promises";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Action =
	| { type: "append"; bytes: number[] }
	| { type: "snapshot"; persist: boolean }
	| { type: "finish" | "close" };
type Case = {
	name: string;
	maxLines: number;
	maxBytes: number;
	actions: Action[];
};
type Snapshot = {
	content: string;
	truncation: unknown;
	fullOutputPath?: string;
};
type Accumulator = {
	append(data: Buffer): void;
	finish(): void;
	snapshot(options: { persistIfTruncated: boolean }): Snapshot;
	closeTempFile(): Promise<void>;
	getLastLineBytes(): number;
};
type Api = {
	OutputAccumulator: new (options: {
		maxLines: number;
		maxBytes: number;
	}) => Accumulator;
};

async function observe(input: Case, api: Api): Promise<unknown> {
	const output = new api.OutputAccumulator(input);
	const frames: unknown[] = [];
	let firstPath: string | undefined;
	for (const action of input.actions) {
		if (action.type === "append") {
			let error: string | null = null;
			try {
				output.append(Buffer.from(action.bytes));
			} catch (thrown) {
				error = thrown instanceof Error ? thrown.message : String(thrown);
			}
			frames.push({
				type: "append",
				error,
				lastLineBytes: output.getLastLineBytes(),
			});
		} else if (action.type === "finish") {
			output.finish();
			frames.push({ type: "finish", lastLineBytes: output.getLastLineBytes() });
		} else if (action.type === "snapshot") {
			const snapshot = output.snapshot({ persistIfTruncated: action.persist });
			firstPath ??= snapshot.fullOutputPath;
			frames.push({
				type: "snapshot",
				content: snapshot.content,
				truncation: snapshot.truncation,
				truncationJson: JSON.stringify(snapshot.truncation),
				hasPath: snapshot.fullOutputPath !== undefined,
				firstPathRetained: snapshot.fullOutputPath === firstPath,
			});
		} else {
			await output.closeTempFile();
			const snapshot = output.snapshot({ persistIfTruncated: false });
			firstPath ??= snapshot.fullOutputPath;
			frames.push({
				type: "close",
				diskHex:
					firstPath === undefined
						? null
						: (await readFile(firstPath)).toString("hex"),
			});
		}
	}
	await output.closeTempFile();
	if (firstPath !== undefined) await unlink(firstPath);
	return { name: input.name, frames };
}

void runOracle(async (module, request) => {
	const api = module as unknown as Api;
	const outputs: unknown[] = [];
	for (const value of requireCases(request))
		outputs.push(await observe(value as Case, api));
	return outputs;
}).catch((error: unknown) => {
	process.stderr.write(
		`accumulator sequence oracle failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
