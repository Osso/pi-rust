import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Entry = { kind: "assistant" | "toolResult"; usage: (number | string)[] };
type Data = { cwd: number[]; name: number[] | null; entries: Entry[] };
type Request = {
	main: Data;
	second: Data;
	child: Data;
	mainModel: unknown;
	secondModel: unknown;
	width: number;
	override?: any;
	env: Record<string, string | null>;
	branch?: string;
	providerCount: number;
	statuses: [number[], number[]][];
	steps: any[];
};
const text = (value: number[]): string => String.fromCharCode(...value);
const units = (value: string): number[] =>
	Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
const number = (value: number | string): number =>
	typeof value === "number" ? value : Number(value);
function entry(value: Entry): unknown {
	const [input, output, cacheRead, cacheWrite, cost] = value.usage.map(number);
	return {
		type: "message",
		message: {
			role: value.kind,
			usage: { input, output, cacheRead, cacheWrite, cost: { total: cost } },
		},
	};
}

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const { FooterComponent } = await import(
		pathToFileURL(`${source}/modes/interactive/components/footer.ts`).href
	);
	const themes = await import(
		pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href
	);
	themes.setThemeInstance(
		new themes.Theme({ dim: 0, error: 0, warning: 0 }, {}, "256color"),
	);
	function run(request: Request): unknown {
		for (const [key, value] of Object.entries(request.env)) {
			if (value === null) delete process.env[key];
			else process.env[key] = value;
		}
		const data = {
			main: request.main,
			second: request.second,
			child: request.child,
		};
		const managers = Object.fromEntries(
			Object.entries(data).map(([key, value]) => [
				key,
				{
					getEntries: () => value.entries.map(entry),
					getSessionName: () =>
						value.name === null ? undefined : text(value.name),
					getCwd: () => text(value.cwd),
				},
			]),
		);
		const session = (name: "main" | "second") => ({
			state: {
				model: name === "main" ? request.mainModel : request.secondModel,
				thinkingLevel: "high",
			},
			sessionManager: managers[name],
			getContextUsage: () => ({ contextWindow: 100_000, percent: 0 }),
			modelRegistry: { isUsingOAuth: () => false },
		});
		const statuses = new Map(
			request.statuses.map(([key, value]) => [text(key), text(value)]),
		);
		const provider = {
			getGitBranch: () => request.branch ?? null,
			getExecutableName: () => "pi-footer-proof",
			getAvailableProviderCount: () => request.providerCount,
			getExtensionStatuses: () => statuses,
		};
		const footer = new FooterComponent(session("main"), provider);
		function override(value: any): void {
			footer.setSessionOverride({
				cwd: text(value.cwd),
				sessionManager:
					value.manager === "none" ? null : managers[value.manager],
				model: value.model,
				thinkingLevel: text(value.thinking),
				contextUsage:
					value.context === null
						? undefined
						: {
								contextWindow: number(value.context.window),
								percent:
									value.context.percent === null
										? null
										: number(value.context.percent),
							},
			});
		}
		if (request.override) override(request.override);
		let width = request.width;
		const snapshot = () => footer.render(width).map(units);
		const output = [snapshot()];
		for (const step of request.steps) {
			switch (step.op) {
				case "append":
					data[step.target as keyof typeof data].entries.push(step.entry);
					break;
				case "name":
					data[step.target as keyof typeof data].name = step.value;
					break;
				case "invalidate":
					footer.invalidate();
					break;
				case "dispose":
					footer.dispose();
					break;
				case "override":
					override(step.value);
					break;
				case "clearOverride":
					footer.clearSessionOverride();
					break;
				case "session":
					footer.setSession(session("second"));
					break;
				case "auto":
					footer.setAutoCompactEnabled(step.value);
					break;
				case "width":
					width = step.value;
					break;
				case "status":
					statuses.set(text(step.key), text(step.value));
					break;
				default:
					throw new Error(`unknown step ${step.op}`);
			}
			output.push(snapshot());
		}
		return output;
	}
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			process.stdout.write(
				`${JSON.stringify({ ok: true, output: run(JSON.parse(line)) })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		}
	}
}
void main();
