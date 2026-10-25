import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Provider = {
	setCwd(cwd: string): void;
	dispose(): void;
	onBranchChange(callback: () => void): () => void;
	setAvailableProviderCount(count: number): void;
	getAvailableProviderCount(): number;
};
const { FooterDataProvider } = (await import(
	pathToFileURL(process.argv[2]).href
)) as { FooterDataProvider: new (cwd: string) => Provider };
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as {
		operation: "numbers" | "live" | "dispose" | "throw";
		cwd: string;
	};
	const provider = new FooterDataProvider(request.cwd);
	const trace: string[] = [];
	let result: unknown;
	try {
		if (request.operation === "numbers") {
			result = [
				0,
				-1.5,
				Number.NaN,
				Number.POSITIVE_INFINITY,
				Number.NEGATIVE_INFINITY,
				2 ** 53,
				-0,
			].map((number) => {
				provider.setAvailableProviderCount(number);
				const value = provider.getAvailableProviderCount();
				return { value, negativeZero: Object.is(value, -0) };
			});
		} else if (request.operation === "live") {
			let removeFirst: (() => void) | undefined;
			let removeSecond: (() => void) | undefined;
			removeFirst = provider.onBranchChange(() => {
				trace.push("first");
				removeFirst?.();
				removeSecond?.();
				provider.onBranchChange(() => trace.push("added"));
			});
			removeSecond = provider.onBranchChange(() => trace.push("removed"));
			provider.setCwd(`${request.cwd}/next`);
			provider.setCwd(`${request.cwd}/again`);
			result = trace;
		} else if (request.operation === "dispose") {
			provider.onBranchChange(() => {
				trace.push("dispose");
				provider.dispose();
			});
			provider.onBranchChange(() => trace.push("removed"));
			provider.setCwd(`${request.cwd}/next`);
			result = trace;
		} else {
			provider.onBranchChange(() => {
				trace.push("throw");
				throw Object.assign(new TypeError("branch-\ud800"), {
					code: "EBRANCH",
				});
			});
			provider.onBranchChange(() => trace.push("removed"));
			try {
				provider.setCwd(`${request.cwd}/next`);
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				result = {
					trace,
					error: {
						name: error.name,
						message: error.message,
						code: Reflect.get(error, "code"),
					},
				};
			}
		}
		process.stdout.write(`${JSON.stringify(result)}\n`);
	} finally {
		provider.dispose();
	}
}
