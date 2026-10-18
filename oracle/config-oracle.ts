// Oracle for `config.ts`: install-method detection, self-update commands, and path getters.
//
// Requests run sequentially. Each request may replace `process.env`, `process.execPath`,
// `process.argv[1]`, and `process.platform` for its duration, which is exactly the surface the
// TypeScript module reads. `PI_PACKAGE_DIR` must also be set on the process for the module-level
// package constants; the differential test spawns one oracle per package fixture.
import { importSource, serve } from "./config-auth-oracle-support.ts";

type InstallMethod = "bun-binary" | "npm" | "pnpm" | "yarn" | "bun" | "unknown";

type ConfigApi = {
	detectInstallMethod(): InstallMethod;
	getSelfUpdateCommand(
		packageName: string,
		npmCommand?: string[],
		target?: string | { packageName: string; installSpec?: string },
	): { command: string; args: string[]; display: string; steps?: unknown } | undefined;
	getUpdateInstruction(packageName: string): string;
	getSelfUpdateUnavailableInstruction(
		packageName: string,
		npmCommand?: string[],
		target?: string | { packageName: string; installSpec?: string },
	): string;
	getPackageDir(): string;
	getThemesDir(): string;
	getExportTemplateDir(): string;
	getPackageJsonPath(): string;
	getReadmePath(): string;
	getDocsPath(): string;
	getExamplesPath(): string;
	getChangelogPath(): string;
	getInteractiveAssetsDir(): string;
	getBundledInteractiveAssetPath(name: string): string;
	getShareViewerUrl(gistId: string): string;
	expandTildePath(path: string): string;
	getUserConfigRoot(): string;
	getUserStateRoot(): string;
	getLegacyAgentDir(): string;
	getAgentDir(): string;
	getCustomThemesDir(): string;
	getModelsPath(): string;
	getAuthPath(): string;
	getSettingsPath(): string;
	getToolsDir(): string;
	getBinDir(): string;
	getPromptsDir(): string;
	getPromptHistoryPath(): string;
	getSessionsDir(): string;
	getDebugLogPath(): string;
	PACKAGE_NAME: string;
	APP_NAME: string;
	APP_TITLE: string;
	CONFIG_DIR_NAME: string;
	VERSION: string;
	ENV_AGENT_DIR: string;
	ENV_SESSION_DIR: string;
	ENV_STATE_DIR: string;
};

type Request = {
	id: unknown;
	op: "paths" | "install";
	env?: Record<string, string>;
	execPath?: string;
	argv1?: string | null;
	platform?: string;
	packageName?: string;
	npmCommand?: string[] | null;
	target?: string | { packageName: string; installSpec?: string } | null;
};

function withProcessState<T>(request: Request, action: () => T): T {
	const envNames = Object.keys(request.env ?? {});
	const previousEnv = new Map<string, string | undefined>();
	for (const name of envNames) {
		previousEnv.set(name, process.env[name]);
		const value = (request.env ?? {})[name];
		if (value === "") delete process.env[name];
		else process.env[name] = value;
	}
	const execDescriptor = Object.getOwnPropertyDescriptor(process, "execPath");
	const platformDescriptor = Object.getOwnPropertyDescriptor(process, "platform");
	const previousArgv1 = process.argv[1];
	if (request.execPath !== undefined) {
		Object.defineProperty(process, "execPath", { value: request.execPath, configurable: true });
	}
	if (request.platform !== undefined) {
		Object.defineProperty(process, "platform", { value: request.platform, configurable: true });
	}
	if (request.argv1 !== undefined) {
		if (request.argv1 === null) process.argv.splice(1, 1);
		else process.argv[1] = request.argv1;
	}
	try {
		return action();
	} finally {
		for (const [name, value] of previousEnv) {
			if (value === undefined) delete process.env[name];
			else process.env[name] = value;
		}
		if (execDescriptor) Object.defineProperty(process, "execPath", execDescriptor);
		if (platformDescriptor) Object.defineProperty(process, "platform", platformDescriptor);
		if (request.argv1 !== undefined) {
			if (previousArgv1 === undefined) process.argv.splice(1, 1);
			else process.argv[1] = previousArgv1;
		}
	}
}

function pathsPayload(api: ConfigApi): Record<string, unknown> {
	return {
		packageDir: api.getPackageDir(),
		packageName: api.PACKAGE_NAME,
		appName: api.APP_NAME,
		appTitle: api.APP_TITLE,
		configDirName: api.CONFIG_DIR_NAME,
		version: api.VERSION,
		envAgentDir: api.ENV_AGENT_DIR,
		envSessionDir: api.ENV_SESSION_DIR,
		envStateDir: api.ENV_STATE_DIR,
		userConfigRoot: api.getUserConfigRoot(),
		userStateRoot: api.getUserStateRoot(),
		legacyAgentDir: api.getLegacyAgentDir(),
		agentDir: api.getAgentDir(),
		customThemesDir: api.getCustomThemesDir(),
		modelsPath: api.getModelsPath(),
		authPath: api.getAuthPath(),
		settingsPath: api.getSettingsPath(),
		toolsDir: api.getToolsDir(),
		binDir: api.getBinDir(),
		promptsDir: api.getPromptsDir(),
		promptHistoryPath: api.getPromptHistoryPath(),
		sessionsDir: api.getSessionsDir(),
		debugLogPath: api.getDebugLogPath(),
		packageJsonPath: api.getPackageJsonPath(),
		readmePath: api.getReadmePath(),
		docsPath: api.getDocsPath(),
		examplesPath: api.getExamplesPath(),
		changelogPath: api.getChangelogPath(),
		shareViewerUrl: api.getShareViewerUrl("abc123"),
		expandedTilde: api.expandTildePath("~/nested/dir"),
	};
}

function installPayload(request: Request, api: ConfigApi): Record<string, unknown> {
	const packageName = request.packageName ?? "@earendil-works/pi-coding-agent";
	const npmCommand = request.npmCommand ?? undefined;
	const target = request.target ?? undefined;
	const command = api.getSelfUpdateCommand(packageName, npmCommand, target);
	return {
		method: api.detectInstallMethod(),
		// `steps` is undefined for a single-step command; normalize to null for the comparison.
		command: command ? { ...command, steps: command.steps ?? null } : null,
		updateInstruction: api.getUpdateInstruction(packageName),
		unavailableInstruction: api.getSelfUpdateUnavailableInstruction(packageName, npmCommand, target),
	};
}

async function main(): Promise<void> {
	const api = await importSource<ConfigApi>();
	serve((request) =>
		withProcessState(request as Request, () =>
			(request as Request).op === "paths" ? pathsPayload(api) : installPayload(request as Request, api),
		),
	);
}

void main().catch((error: unknown) => {
	process.stderr.write(`config oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
