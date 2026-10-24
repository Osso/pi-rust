import { appendFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

type Rejection =
	| { kind: "raw"; value: unknown }
	| {
			kind: "error";
			name: "Error" | "TypeError";
			message: string;
			code?: string;
	  };
type Config = {
	capture?: boolean;
	traceDispose?: boolean;
	disposeRejection?: Rejection;
	promptRejection?: Rejection;
};
type Runtime = {
	setBeforeSessionInvalidate(handler: () => void): void;
	session: {
		setWorkingDirectoryRecoveryHandlers(handlers: {
			beforeRun(): Promise<never>;
			afterTurn(): Promise<void>;
		}): void;
	};
};

function makeRejection(value: Rejection): unknown {
	if (value.kind === "raw") return value.value;
	const error =
		value.name === "TypeError"
			? new TypeError(value.message)
			: new Error(value.message);
	if (value.code !== undefined) Object.assign(error, { code: value.code });
	return error;
}

function serializeRejection(error: unknown) {
	if (!(error instanceof Error)) return { kind: "raw", value: error };
	return {
		kind: "error",
		value: Object.assign({ name: error.name, message: error.message }, error),
		typeError: error instanceof TypeError,
	};
}

export function markPrintPending() {
	if (process.env.PI_HARNESS_PRINT_FIXTURE === undefined) return;
	const root = process.env.PI_HARNESS_PRINT_ROOT;
	if (root === undefined) throw new Error("print fixture root required");
	// A live provider holds a socket; this private pending factory holds a timer instead.
	setInterval(() => {}, 1000);
	writeFileSync(
		join(root, "print-ready.pending"),
		JSON.stringify({ pid: process.pid }),
	);
	renameSync(join(root, "print-ready.pending"), join(root, "print-ready.json"));
}

export function configurePrintFixture(runtime: Runtime) {
	const encoded = process.env.PI_HARNESS_PRINT_FIXTURE;
	if (encoded === undefined) return undefined;
	const root = process.env.PI_HARNESS_PRINT_ROOT;
	if (root === undefined) throw new Error("print fixture root required");
	const config = JSON.parse(encoded) as Config;
	if (config.promptRejection !== undefined) {
		const rejection = makeRejection(config.promptRejection);
		runtime.session.setWorkingDirectoryRecoveryHandlers({
			beforeRun: async () => {
				throw rejection;
			},
			afterTurn: async () => {},
		});
	}
	if (config.disposeRejection !== undefined || config.traceDispose === true) {
		const rejection =
			config.disposeRejection === undefined
				? undefined
				: makeRejection(config.disposeRejection);
		runtime.setBeforeSessionInvalidate(() => {
			appendFileSync(
				join(root, "print-trace.jsonl"),
				`${JSON.stringify({ event: "beforeInvalidate" })}\n`,
			);
			if (config.disposeRejection !== undefined) throw rejection;
		});
	}
	return {
		capture: config.capture === true,
		returned(code: number) {
			writeFileSync(
				join(root, "print-report.json"),
				JSON.stringify({ returned: code }),
			);
		},
		rejected(error: unknown) {
			writeFileSync(
				join(root, "print-report.json"),
				JSON.stringify({ rejected: serializeRejection(error) }),
			);
		},
	};
}
