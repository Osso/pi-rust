import { importSource, serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

type Event = { type: string; source: string; action: string; message?: string };
type Manager = { setProgressCallback(callback: ((event: Event) => void) | undefined): void; install(source: string): Promise<void> };
type Source = { DefaultPackageManager: new (options: unknown) => Manager };
function rejection(descriptor: Record<string, unknown>): unknown {
	if (descriptor.kind === "native") return Object.assign(new TypeError(descriptor.message as string, { cause: false }), { code: "E_PROGRESS" });
	return descriptor.value;
}
async function main(): Promise<void> {
	const { DefaultPackageManager } = await importSource<Source>();
	serve(async request => {
		const settingsManager = { getGlobalSettings: () => ({}), getProjectSettings: () => ({}), isProjectTrusted: () => false };
		const manager = new DefaultPackageManager({ cwd: request.base, agentDir: request.base, settingsManager });
		const progress: Event[] = [];
		const failure = rejection(request.failure as Record<string, unknown>);
		manager.setProgressCallback(event => {
			progress.push(event);
			if (request.stage === "clear") manager.setProgressCallback(undefined);
			if (event.type === request.stage) throw failure;
		});
		try {
			await manager.install(request.source as string);
			return { outcome: { ok: true }, progress };
		} catch (error) {
			return { outcome: { ok: false, error: describeAbortReason(error) }, progress };
		}
	});
}
void main();
