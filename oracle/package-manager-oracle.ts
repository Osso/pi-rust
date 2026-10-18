import { StringDecoder } from "node:string_decoder";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type PackageSource = string | Record<string, unknown>;

type SettingsSpec = {
	packages?: PackageSource[];
	extensions?: string[];
	skills?: string[];
	prompts?: string[];
	themes?: string[];
	npmCommand?: string[];
};

type ScenarioSpec = {
	cwd: string;
	agentDir: string;
	global?: SettingsSpec;
	project?: SettingsSpec;
	trusted?: boolean;
};

type Request = {
	id: RequestId;
	operation: string;
	scenario?: ScenarioSpec;
	source?: string;
	scope?: "user" | "project";
	local?: boolean;
	sources?: string[];
	temporary?: boolean;
};

type ProgressEvent = {
	type: string;
	action: string;
	actionSource: string;
	message?: string;
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

type PackageManagerApi = {
	DefaultPackageManager: new (options: {
		cwd: string;
		agentDir: string;
		settingsManager: unknown;
	}) => Record<string, (...args: unknown[]) => unknown>;
};

type SettingsApi = {
	SettingsManager: {
		inMemory(settings: SettingsSpec, options?: { projectTrusted?: boolean }): {
			setProjectPackages(packages: PackageSource[]): void;
			setProjectExtensionPaths(paths: string[]): void;
			setProjectSkillPaths(paths: string[]): void;
			setProjectPromptTemplatePaths(paths: string[]): void;
			setProjectThemePaths(paths: string[]): void;
			setProjectTrusted(trusted: boolean): void;
			getGlobalSettings(): SettingsSpec;
			getProjectSettings(): SettingsSpec;
		};
	};
};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("package manager oracle requires a source module path");
	}
	return sourceModulePath;
}

function createSettings(settingsApi: SettingsApi, scenario: ScenarioSpec): unknown {
	const globalSettings = { ...(scenario.global ?? {}) };
	const settingsManager = settingsApi.SettingsManager.inMemory(globalSettings, { projectTrusted: true });
	const project = scenario.project ?? {};
	if (project.packages) settingsManager.setProjectPackages(project.packages);
	if (project.extensions) settingsManager.setProjectExtensionPaths(project.extensions);
	if (project.skills) settingsManager.setProjectSkillPaths(project.skills);
	if (project.prompts) settingsManager.setProjectPromptTemplatePaths(project.prompts);
	if (project.themes) settingsManager.setProjectThemePaths(project.themes);
	settingsManager.setProjectTrusted(scenario.trusted === true);
	return settingsManager;
}

function createManager(api: PackageManagerApi, settingsApi: SettingsApi, scenario: ScenarioSpec) {
	const settingsManager = createSettings(settingsApi, scenario);
	const manager = new api.DefaultPackageManager({
		cwd: scenario.cwd,
		agentDir: scenario.agentDir,
		settingsManager,
	});
	const progress: ProgressEvent[] = [];
	manager.setProgressCallback((event: ProgressEvent) => {
		progress.push(event);
	});
	return { manager, progress, settingsManager };
}

function settingsSnapshot(settings: SettingsSpec): unknown {
	return {
		packages: settings.packages ?? [],
		extensions: settings.extensions ?? [],
		skills: settings.skills ?? [],
		prompts: settings.prompts ?? [],
		themes: settings.themes ?? [],
		npmCommand: settings.npmCommand ?? null,
	};
}

async function execute(request: Request, api: PackageManagerApi, settingsApi: SettingsApi): Promise<unknown> {
	const scenario = request.scenario;
	if (!scenario) {
		throw new Error("scenario is required");
	}
	const { manager, progress, settingsManager } = createManager(api, settingsApi, scenario);
	const anyManager = manager as Record<string, (...args: unknown[]) => unknown>;
	switch (request.operation) {
		case "parse_source":
			return { parsed: anyManager.parseSource(request.source) };
		case "identity":
			return { identity: anyManager.getPackageIdentity(request.source) };
		case "source_match_key_for_input":
			return { key: anyManager.getSourceMatchKeyForInput(request.source) };
		case "normalize_package_source_for_settings":
			return { normalized: anyManager.normalizePackageSourceForSettings(request.source, request.local ? "project" : "user") };
		case "npm_install_path":
			return {
				path: anyManager.getNpmInstallPath(
					anyManager.parseSource(request.source),
					request.scope ?? "user",
				),
			};
		case "git_install_path":
			return {
				path: anyManager.getGitInstallPath(
					anyManager.parseSource(request.source),
					request.scope ?? "user",
				),
			};
		case "get_installed_path":
			return { path: manager.getInstalledPath(request.source as string, request.scope ?? "user") ?? null };
		case "list_configured_packages":
			return { packages: manager.listConfiguredPackages() };
		case "resolve": {
			const resolved = await (manager.resolve as (...args: unknown[]) => Promise<unknown>)();
			return { resolved, progress };
		}
		case "resolve_extension_sources": {
			const resolved = await (
				manager.resolveExtensionSources as (...args: unknown[]) => Promise<unknown>
			)(request.sources, { local: request.local, temporary: request.temporary });
			return { resolved, progress };
		}
		case "install": {
			await (manager.install as (...args: unknown[]) => Promise<void>)(request.source, { local: request.local });
			return {
				progress,
				global: settingsSnapshot(settingsManager.getGlobalSettings()),
				project: settingsSnapshot(settingsManager.getProjectSettings()),
			};
		}
		case "install_and_persist": {
			await (manager.installAndPersist as (...args: unknown[]) => Promise<void>)(request.source, {
				local: request.local,
			});
			return {
				progress,
				global: settingsSnapshot(settingsManager.getGlobalSettings()),
				project: settingsSnapshot(settingsManager.getProjectSettings()),
			};
		}
		case "remove": {
			await (manager.remove as (...args: unknown[]) => Promise<void>)(request.source, { local: request.local });
			return {
				progress,
				global: settingsSnapshot(settingsManager.getGlobalSettings()),
				project: settingsSnapshot(settingsManager.getProjectSettings()),
			};
		}
		case "remove_and_persist": {
			const removed = await (manager.removeAndPersist as (...args: unknown[]) => Promise<boolean>)(request.source, {
				local: request.local,
			});
			return {
				removed,
				progress,
				global: settingsSnapshot(settingsManager.getGlobalSettings()),
				project: settingsSnapshot(settingsManager.getProjectSettings()),
			};
		}
		case "update": {
			await (manager.update as (...args: unknown[]) => Promise<void>)(request.source);
			return { progress };
		}
		case "check_for_available_updates": {
			return { updates: await (manager.checkForAvailableUpdates as () => Promise<unknown>)() };
		}
		case "add_source_to_settings": {
			const changed = manager.addSourceToSettings(request.source as string, { local: request.local }) as boolean;
			return {
				changed,
				global: settingsSnapshot(settingsManager.getGlobalSettings()),
				project: settingsSnapshot(settingsManager.getProjectSettings()),
			};
		}
		case "remove_source_from_settings": {
			const changed = manager.removeSourceFromSettings(request.source as string, {
				local: request.local,
			}) as boolean;
			return {
				changed,
				global: settingsSnapshot(settingsManager.getGlobalSettings()),
				project: settingsSnapshot(settingsManager.getProjectSettings()),
			};
		}
		default:
			throw new Error(`unknown operation: ${request.operation}`);
	}
}

async function executeRequestLine(
	line: string,
	api: PackageManagerApi,
	settingsApi: SettingsApi,
): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const output = await execute(request, api, settingsApi);
		process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(api: PackageManagerApi, settingsApi: SettingsApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let queue = Promise.resolve();
	const drain = () => {
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			queue = queue.then(() => executeRequestLine(line, api, settingsApi));
		}
	};
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		drain();
	});
	process.stdin.on("end", () => {
		void queue;
	});
}

async function main(): Promise<void> {
	const sourceModulePath = readSourceModulePath();
	const api = (await import(pathToFileURL(sourceModulePath).href)) as unknown as PackageManagerApi;
	const settingsModulePath = join(dirname(sourceModulePath), "settings-manager.ts");
	const settingsApi = (await import(pathToFileURL(settingsModulePath).href)) as unknown as SettingsApi;
	serveRequests(api, settingsApi);
}

void main().catch((error: unknown) => {
	process.stderr.write(`package manager oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
