import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

async function main(): Promise<void> {
	const api = (await import(process.argv[2] ?? "")) as unknown as {
		PyrunRunnerClient: new (options?: unknown) => {
			evaluate(params: unknown, onProgress?: unknown, signal?: unknown, onPiRequest?: unknown): Promise<unknown>;
			dispose(): void;
		};
	};
	await runOracle((_module, request) => {
		const cases = requireCases(request);
		return Promise.all(
			cases.map(async (value) => {
				const input = value as { params: unknown; options?: unknown };
				const runner = new api.PyrunRunnerClient(input.options);
				try {
					return await runner.evaluate(input.params);
				} finally {
					runner.dispose();
				}
			}),
		);
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`coding-agent pyrun oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
