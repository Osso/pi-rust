import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(
	pathToFileURL(`${source}/modes/interactive/interactive-mode.ts`).href
);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as {
			operation: "cd" | "resume";
			cwd: string;
			target: string;
			file: string;
			command?: string;
		};
		let context: { cwd: string; mode: string; hasUI: boolean } | undefined;
		const receiver = Object.create(InteractiveMode.prototype);
		receiver.runtimeHost = {
			session: { sessionManager: { getCwd: () => request.cwd } },
			relocate: async (
				_path: string,
				options: { projectTrustContext: typeof context },
			) => {
				context = options.projectTrustContext;
			},
			switchSession: async (
				_path: string,
				options: {
					projectTrustContextFactory: (
						cwd: string,
					) => NonNullable<typeof context>;
				},
			) => {
				context = options.projectTrustContextFactory(request.target);
				return { cancelled: false };
			},
		};
		receiver.createExtensionUIContext = () => ({
			select: () => {},
			confirm: () => {},
			input: () => {},
			notify: () => {},
		});
		receiver.statusContainer = { clear: () => {} };
		receiver.renderCurrentSessionState = () => {};
		receiver.continueInterruptedResumedSession = async () => {};
		receiver.showStatus = () => {};
		receiver.showError = (message: string) => {
			throw new Error(message);
		};
		if (request.operation === "cd")
			await receiver.handleCdCommand(
				`/cd ${request.command ?? request.target}`,
			);
		else await receiver.handleResumeSession(request.file);
		console.log(
			JSON.stringify({
				context: context
					? { cwd: context.cwd, mode: context.mode, hasUI: context.hasUI }
					: null,
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ error: String(error) }));
	}
}
