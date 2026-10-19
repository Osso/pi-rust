import { createInterface } from "node:readline";

type RequestId = string | number | null;

type UiApi = {
	runWithProgress<T>(
		ui: unknown,
		options: {
			title: string;
			model: string;
			initialMessage: string;
			cancelTitle: string;
			cancelMessage: string;
			run(signal: AbortSignal, update: (progress: unknown) => void): Promise<T>;
			cancel(): Promise<void>;
		},
	): Promise<{ cancelled: true } | { cancelled: false; value: T }>;
};

type Scenario = "complete" | "error" | "cancel" | "not_stop";

type Request = { id: RequestId; op: "run_with_progress"; scenario: Scenario };

function errorMessage(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

function stateJson(state: {
	title: string;
	model: string;
	message: string;
	ratio?: number;
	detail?: string;
}): unknown {
	return {
		title: state.title,
		model: state.model,
		message: state.message,
		ratio: state.ratio ?? null,
		detail: state.detail ?? null,
	};
}

async function runWithProgress(api: UiApi, scenario: Scenario): Promise<unknown> {
	const updates: unknown[] = [];
	let cancelCalls = 0;
	const ui = {
		updateProgress: (state: { title: string; model: string; message: string; ratio?: number; detail?: string }) => {
			updates.push(stateJson(state));
		},
		progress: () => {
			if (scenario === "complete" || scenario === "error") return new Promise<void>(() => {});
			if (scenario === "not_stop") return new Promise<void>((resolve) => setTimeout(resolve, 0));
			return Promise.resolve();
		},
		confirm: async () => scenario === "cancel",
		select: async () => undefined,
		showModels: async () => ({ type: "close" }),
		connectionError: async () => "close",
		searchModels: async () => undefined,
		showStatus: () => {},
	};
	const options = {
		title: "Loading model",
		model: "test-model",
		initialMessage: "Starting…",
		cancelTitle: "Stop loading?",
		cancelMessage: "test-model",
		run: async (signal: AbortSignal, update: (progress: unknown) => void) => {
			update({ message: "Half", ratio: 0.5 });
			if (scenario === "error") throw new Error("boom");
			if (scenario === "cancel") {
				await new Promise((_resolve, reject) => {
					signal.addEventListener("abort", () => reject(signal.reason), { once: true });
				});
			}
			if (scenario === "not_stop") await new Promise((resolve) => setTimeout(resolve, 10));
			return "done";
		},
		cancel: async () => {
			cancelCalls += 1;
		},
	};
	try {
		const outcome = await api.runWithProgress(ui, options);
		return { outcome, updates, cancelCalls };
	} catch (error) {
		return { error: errorMessage(error), updates, cancelCalls };
	}
}

async function execute(request: Request, api: UiApi): Promise<unknown> {
	return runWithProgress(api, request.scenario);
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (!modulePath) throw new Error("oracle requires the source module path");
	const api = (await import(modulePath)) as UiApi;
	const lines = createInterface({ input: process.stdin });
	for await (const line of lines) {
		if (!line.trim()) continue;
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const output = await execute(request, api);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: errorMessage(error) })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`${String(error)}\n`);
	process.exitCode = 1;
});
