import { readFile, unlink } from "node:fs/promises";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Limit = number | "NaN" | "Infinity" | "-Infinity" | "-0";
type Snapshot = {
	content: string;
	truncation: { maxLines: number; maxBytes: number; [key: string]: unknown };
	fullOutputPath?: string;
};

type Case = {
	name: string;
	maxLines?: Limit;
	maxBytes?: Limit;
	chunks: number[][];
	finish: boolean;
	persistIfTruncated: boolean;
	appendAfterFinish?: boolean;
};

type Api = {
	OutputAccumulator: new (options?: {
		maxLines?: number;
		maxBytes?: number;
		tempFilePrefix?: string;
	}) => {
		append(data: Uint8Array): void;
		finish(): void;
		snapshot(options?: { persistIfTruncated?: boolean }): Snapshot;
		closeTempFile(): Promise<void>;
		getLastLineBytes(): number;
	};
};

function encodeLimit(value: number): Limit {
	if (Object.is(value, -0)) return "-0";
	if (Number.isNaN(value)) return "NaN";
	if (value === Infinity) return "Infinity";
	if (value === -Infinity) return "-Infinity";
	return value;
}

void runOracle(async (module, request) => {
	const api = module as unknown as Api;
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		const accumulator = new api.OutputAccumulator({
			maxLines:
				input.maxLines === undefined ? undefined : Number(input.maxLines),
			maxBytes:
				input.maxBytes === undefined ? undefined : Number(input.maxBytes),
			tempFilePrefix: "pi-output-diff",
		});
		const lastLineBytes: number[] = [];
		for (const chunk of input.chunks) {
			accumulator.append(Buffer.from(chunk));
			lastLineBytes.push(accumulator.getLastLineBytes());
		}
		if (input.finish) {
			accumulator.finish();
			lastLineBytes.push(accumulator.getLastLineBytes());
		}
		let appendError: string | null = null;
		if (input.appendAfterFinish) {
			try {
				accumulator.append(Buffer.from([0x61]));
			} catch (error) {
				appendError = error instanceof Error ? error.message : String(error);
			}
		}
		const snapshot = accumulator.snapshot({
			persistIfTruncated: input.persistIfTruncated,
		});
		let tempFileHex: string | null = null;
		if (snapshot.fullOutputPath !== undefined) {
			await accumulator.closeTempFile();
			tempFileHex = (await readFile(snapshot.fullOutputPath)).toString("hex");
			await unlink(snapshot.fullOutputPath);
		}
		results.push({
			name: input.name,
			content: snapshot.content,
			truncation: snapshot.truncation,
			truncationJson: JSON.stringify(snapshot.truncation),
			limits: {
				maxLines: encodeLimit(snapshot.truncation.maxLines),
				maxBytes: encodeLimit(snapshot.truncation.maxBytes),
			},
			hasFullOutputPath: snapshot.fullOutputPath !== undefined,
			tempFileHex,
			lastLineBytes,
			appendError,
		});
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(
		`coding-agent output-accumulator oracle startup failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
