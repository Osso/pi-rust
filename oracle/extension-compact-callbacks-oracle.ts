import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Options = {
	customInstructions: string;
	onComplete(result: { summary: string }): void;
	onError(error: unknown): void;
};
type Actions = { compact(options: Options): void };
type Session = {
	compact(instructions: string): Promise<{ summary: string }>;
	_bindExtensionCore(runner: {
		bindCore(actions: unknown, context: Actions, providers: unknown): void;
	}): void;
};
function snapshot(error: unknown) {
	if (!(error instanceof Error)) return { kind: "value", value: error };
	return {
		kind: "error",
		value: {
			name: error.name,
			message: error.message,
			...Object.fromEntries(
				Object.entries(error).filter(([key]) => key !== "stack"),
			),
		},
	};
}

void runOracle(async (module, request) => {
	const api = module as { AgentSession: { prototype: Session } };
	const events: unknown[] = [];
	let actions: Actions | undefined;
	const session = Object.create(api.AgentSession.prototype) as Session;
	session.compact = async (instructions) => {
		events.push(["produce", instructions]);
		return { summary: "callback summary" };
	};
	session._bindExtensionCore({
		bindCore: (_actions, context) => {
			actions = context;
		},
	});
	if (!actions)
		throw new Error("actual AgentSession binding did not provide compact");
	let finish: () => void = () => {};
	const settled = new Promise<void>((resolve) => {
		finish = resolve;
	});
	const escaped = (error: unknown) => {
		events.push(["unhandled", snapshot(error)]);
		finish();
	};
	process.once("unhandledRejection", escaped);
	try {
		actions.compact({
			customInstructions: "callback instructions",
			onComplete: (result) => {
				events.push(["complete", result.summary]);
				if (request.mode !== "success")
					throw Object.assign(new TypeError(request.completeError as string), {
						code: "E_COMPLETE",
					});
				finish();
			},
			onError: (error) => {
				events.push(["error", snapshot(error)]);
				if (request.mode === "errorThrow")
					throw Object.assign(new RangeError(request.handlerError as string), {
						code: "E_HANDLER",
					});
				finish();
			},
		});
		await settled;
		return events;
	} finally {
		process.removeListener("unhandledRejection", escaped);
	}
}).catch((error: unknown) => {
	process.stderr.write(`compact callbacks oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
