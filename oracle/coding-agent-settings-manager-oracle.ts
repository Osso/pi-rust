import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type SettingsManagerApi = {
	SettingsManager: {
		create(projectDir: string, agentDir: string): SettingsManagerInstance;
	};
};

type SettingsManagerInstance = Record<string, (...args: any[]) => any>;

type Step = { op: string; [key: string]: any };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

async function applyStep(manager: SettingsManagerInstance, step: Step, agentDir: string, projectDir: string): Promise<{ threw: boolean }> {
	try {
		switch (step.op) {
			case "set_last_changelog_version": manager.setLastChangelogVersion(step.value); break;
			case "set_enable_analytics": manager.setEnableAnalytics(step.value); break;
			case "set_theme": manager.setTheme(step.value); break;
			case "set_default_provider": manager.setDefaultProvider(step.value); break;
			case "set_default_model": manager.setDefaultModel(step.value); break;
			case "set_default_model_and_provider": manager.setDefaultModelAndProvider(step.provider, step.model); break;
			case "set_default_thinking_level": manager.setDefaultThinkingLevel(step.value); break;
			case "set_steering_mode": manager.setSteeringMode(step.value); break;
			case "set_follow_up_mode": manager.setFollowUpMode(step.value); break;
			case "set_transport": manager.setTransport(step.value); break;
			case "set_approval_policy": manager.setApprovalPolicy(step.value); break;
			case "set_approval_preset": manager.setApprovalPreset(step.value, step.scope); break;
			case "set_sandbox_profile": manager.setSandboxProfile(step.value, step.scope); break;
			case "set_session_sandbox_profile": manager.setSessionSandboxProfile(step.value ?? undefined); break;
			case "set_project_trusted": manager.setProjectTrusted(step.value); break;
			case "set_compaction_enabled": manager.setCompactionEnabled(step.value); break;
			case "set_retry_enabled": manager.setRetryEnabled(step.value); break;
			case "set_http_idle_timeout_ms": manager.setHttpIdleTimeoutMs(step.value); break;
			case "set_hide_tool_output": manager.setHideToolOutput(step.value); break;
			case "set_hide_coordination_messages": manager.setHideCoordinationMessages(step.value); break;
			case "set_hide_thinking_block": manager.setHideThinkingBlock(step.value); break;
			case "set_shell_path": manager.setShellPath(step.value ?? undefined); break;
			case "set_quiet_startup": manager.setQuietStartup(step.value); break;
			case "set_default_project_trust": manager.setDefaultProjectTrust(step.value); break;
			case "set_shell_command_prefix": manager.setShellCommandPrefix(step.value ?? undefined); break;
			case "set_npm_command": manager.setNpmCommand(step.value ?? undefined); break;
			case "set_collapse_changelog": manager.setCollapseChangelog(step.value); break;
			case "set_enable_install_telemetry": manager.setEnableInstallTelemetry(step.value); break;
			case "set_packages": manager.setPackages(step.value); break;
			case "roundtrip_packages": manager.setPackages(manager.getPackages()); break;
			case "set_project_packages": manager.setProjectPackages(step.value); break;
			case "set_extension_paths": manager.setExtensionPaths(step.value); break;
			case "set_project_extension_paths": manager.setProjectExtensionPaths(step.value); break;
			case "set_skill_paths": manager.setSkillPaths(step.value); break;
			case "set_project_skill_paths": manager.setProjectSkillPaths(step.value); break;
			case "set_prompt_template_paths": manager.setPromptTemplatePaths(step.value); break;
			case "set_project_prompt_template_paths": manager.setProjectPromptTemplatePaths(step.value); break;
			case "set_theme_paths": manager.setThemePaths(step.value); break;
			case "set_project_theme_paths": manager.setProjectThemePaths(step.value); break;
			case "set_enable_skill_commands": manager.setEnableSkillCommands(step.value); break;
			case "set_show_images": manager.setShowImages(step.value); break;
			case "set_image_width_cells": manager.setImageWidthCells(step.value); break;
			case "set_clear_on_shrink": manager.setClearOnShrink(step.value); break;
			case "set_show_terminal_progress": manager.setShowTerminalProgress(step.value); break;
			case "set_image_auto_resize": manager.setImageAutoResize(step.value); break;
			case "set_block_images": manager.setBlockImages(step.value); break;
			case "set_enabled_models": manager.setEnabledModels(step.value ?? undefined, step.scope); break;
			case "set_double_escape_action": manager.setDoubleEscapeAction(step.value); break;
			case "set_tree_filter_mode": manager.setTreeFilterMode(step.value); break;
			case "set_show_hardware_cursor": manager.setShowHardwareCursor(step.value); break;
			case "set_editor_padding_x": manager.setEditorPaddingX(step.value); break;
			case "set_output_pad": manager.setOutputPad(step.value); break;
			case "set_autocomplete_max_visible": manager.setAutocompleteMaxVisible(step.value); break;
			case "set_warnings": manager.setWarnings(step.value); break;
			case "apply_overrides": manager.applyOverrides(step.value); break;
			case "reload": await manager.reload(); break;
			case "flush": await manager.flush(); break;
			case "project_write_then_revoke_same_turn": {
				manager.setProjectExtensionPaths(step.value);
				manager.setProjectTrusted(false);
				await manager.flush();
				break;
			}
			case "setter_external_write_same_turn": {
				manager.setTheme(step.value);
				writeFileSync(join(agentDir, "settings.json"), step.content, "utf-8");
				await manager.flush();
				break;
			}
			case "fixture_barrier": {
				writeFileSync(join(agentDir, "barrier-ready"), "ready");
				const deadline = Date.now() + 5000;
				while (!existsSync(join(agentDir, "barrier-release"))) {
					if (Date.now() >= deadline) throw new Error("fixture barrier timed out");
					await new Promise((resolve) => setTimeout(resolve, 1));
				}
				break;
			}
			case "external_edit": {
				const path = step.scope === "project" ? join(projectDir, ".pi", "settings.json") : join(agentDir, "settings.json");
				writeFileSync(path, step.content, "utf-8");
				break;
			}
			default:
				throw new Error(`unknown step: ${step.op}`);
		}
		return { threw: false };
	} catch {
		return { threw: true };
	}
}

function snapshot(manager: SettingsManagerInstance): unknown {
	const safe = (fn: () => unknown): unknown => {
		try {
			const value = fn();
			return value === undefined ? null : value;
		} catch {
			return "ERROR";
		}
	};
	return {
		globalSettings: manager.getGlobalSettings(),
		projectSettings: manager.getProjectSettings(),
		mergedSettings: manager.getMergedSettings(),
		projectTrusted: manager.isProjectTrusted(),
		lastChangelogVersion: safe(() => manager.getLastChangelogVersion()),
		externalEditorCommand: safe(() => manager.getExternalEditorCommand()),
		theme: safe(() => manager.getTheme()),
		themeSetting: safe(() => manager.getThemeSetting()),
		defaultProvider: safe(() => manager.getDefaultProvider()),
		defaultModel: safe(() => manager.getDefaultModel()),
		defaultThinkingLevel: safe(() => manager.getDefaultThinkingLevel()),
		steeringMode: safe(() => manager.getSteeringMode()),
		followUpMode: safe(() => manager.getFollowUpMode()),
		transport: safe(() => manager.getTransport()),
		approvalPolicy: safe(() => manager.getApprovalPolicy()),
		approvalPreset: safe(() => manager.getApprovalPreset()),
		sandboxProfile: safe(() => manager.getSandboxProfile()),
		configuredSandboxProfile: safe(() => manager.getConfiguredSandboxProfile()),
		explicitSandboxProfile: safe(() => manager.getExplicitSandboxProfile()),
		sessionSandboxProfile: safe(() => manager.getSessionSandboxProfile()),
		compactionEnabled: safe(() => manager.getCompactionEnabled()),
		compactionReserveTokens: safe(() => manager.getCompactionReserveTokens()),
		compactionKeepRecentTokens: safe(() => manager.getCompactionKeepRecentTokens()),
		branchSummarySkipPrompt: safe(() => manager.getBranchSummarySkipPrompt()),
		retryEnabled: safe(() => manager.getRetryEnabled()),
		compaction: safe(() => manager.getCompactionSettings()),
		branchSummary: safe(() => manager.getBranchSummarySettings()),
		retry: safe(() => manager.getRetrySettings()),
		providerRetry: safe(() => manager.getProviderRetrySettings()),
		httpIdleTimeoutMs: safe(() => manager.getHttpIdleTimeoutMs()),
		websocketConnectTimeoutMs: safe(() => manager.getWebSocketConnectTimeoutMs()),
		hideThinkingBlock: safe(() => manager.getHideThinkingBlock()),
		hideToolOutput: safe(() => manager.getHideToolOutput()),
		hideCoordinationMessages: safe(() => manager.getHideCoordinationMessages()),
		idleCompaction: safe(() => manager.getIdleCompactionEnabled()),
		shellPath: safe(() => manager.getShellPath()),
		sessionDir: safe(() => manager.getSessionDir()),
		quietStartup: safe(() => manager.getQuietStartup()),
		defaultProjectTrust: safe(() => manager.getDefaultProjectTrust()),
		shellCommandPrefix: safe(() => manager.getShellCommandPrefix()),
		npmCommand: safe(() => manager.getNpmCommand()),
		collapseChangelog: safe(() => manager.getCollapseChangelog()),
		enableInstallTelemetry: safe(() => manager.getEnableInstallTelemetry()),
		enableAnalytics: safe(() => manager.getEnableAnalytics()),
		hasTrackingId: safe(() => manager.getTrackingId() !== undefined),
		packages: safe(() => manager.getPackages()),
		extensionPaths: safe(() => manager.getExtensionPaths()),
		disabledExtensions: safe(() => manager.getDisabledExtensions()),
		skillPaths: safe(() => manager.getSkillPaths()),
		promptTemplatePaths: safe(() => manager.getPromptTemplatePaths()),
		themePaths: safe(() => manager.getThemePaths()),
		enableSkillCommands: safe(() => manager.getEnableSkillCommands()),
		thinkingBudgets: safe(() => manager.getThinkingBudgets()),
		showImages: safe(() => manager.getShowImages()),
		imageWidthCells: safe(() => manager.getImageWidthCells()),
		clearOnShrink: safe(() => manager.getClearOnShrink()),
		showTerminalProgress: safe(() => manager.getShowTerminalProgress()),
		imageAutoResize: safe(() => manager.getImageAutoResize()),
		blockImages: safe(() => manager.getBlockImages()),
		enabledModels: safe(() => manager.getEnabledModels()),
		doubleEscapeAction: safe(() => manager.getDoubleEscapeAction()),
		treeFilterMode: safe(() => manager.getTreeFilterMode()),
		showHardwareCursor: safe(() => manager.getShowHardwareCursor()),
		editorPaddingX: safe(() => manager.getEditorPaddingX()),
		outputPad: safe(() => manager.getOutputPad()),
		autocompleteMaxVisible: safe(() => manager.getAutocompleteMaxVisible()),
		codeBlockIndent: safe(() => manager.getCodeBlockIndent()),
		warnings: safe(() => manager.getWarnings()),
		permissionPromptTool: safe(() => manager.getPermissionPromptTool()),
		agentProfiles: {
			browser: safe(() => manager.getAgentProfile("browser")),
			explore: safe(() => manager.getAgentProfile("explore")),
			implement: safe(() => manager.getAgentProfile("implement")),
			verifier: safe(() => manager.getAgentProfile("verifier")),
			"documentation-update": safe(() => manager.getAgentProfile("documentation-update")),
			reviewer: safe(() => manager.getAgentProfile("reviewer")),
			unknown: safe(() => manager.getAgentProfile("unknown")),
			empty: safe(() => manager.getAgentProfile("   ")),
		},
	};
}

function readFile(path: string): string | null {
	return existsSync(path) ? readFileSync(path, "utf-8") : null;
}

// Optional handles keep stale managers alive across competing Rust writes.
const managers = new Map<string, SettingsManagerInstance>();

async function execute(request: {
	id: RequestId;
	agentDir: string;
	projectDir: string;
	steps: Step[];
	managerKey?: string;
}, api: SettingsManagerApi): Promise<unknown> {
	const manager = (request.managerKey ? managers.get(request.managerKey) : undefined)
		?? api.SettingsManager.create(request.projectDir, request.agentDir);
	if (request.managerKey) managers.set(request.managerKey, manager);
	const snapshots: unknown[] = [];
	for (const step of request.steps) {
		const result = await applyStep(manager, step, request.agentDir, request.projectDir);
		// File bytes are compared after the queued write settles (the Rust port writes synchronously).
		await manager.flush();
		snapshots.push({
			threw: result.threw,
			snapshot: snapshot(manager),
			globalFile: readFile(join(request.agentDir, "settings.json")),
			projectFile: readFile(join(request.projectDir, ".pi", "settings.json")),
		});
	}
	const errors = manager.drainErrors().map((error: { scope: string }) => error.scope);
	return { snapshots, errors };
}

function serveRequests(api: SettingsManagerApi): void {
	let buffer = "";
	let chain = Promise.resolve();
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8");
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			chain = chain.then(() => executeRequestLine(line, api));
		}
	});
}

async function executeRequestLine(line: string, api: SettingsManagerApi): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as { id: RequestId; agentDir: string; projectDir: string; steps: Step[] };
		id = request.id;
		const response: Response = { id, ok: true, output: await execute(request, api) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("settings manager oracle requires a source module path");
	}
	const api = (await import(pathToFileURL(sourceModulePath).href)) as SettingsManagerApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`settings manager oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
