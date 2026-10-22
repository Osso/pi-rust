import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import path from "node:path";

async function captureStatus(source: string, units: number[]): Promise<unknown> {
	const module = await import(pathToFileURL(path.join(source, "modes/interactive/components/status-indicator.ts")).href);
	const raw = String.fromCharCode(...units);
	let requests = 0;
	const callbacks: string[] = [];
	const ui = {
		requestRender: () => requests++,
		requestComponentRender: () => requests++,
	};
	const colorMessage = (text: string): string => {
		callbacks.push(text);
		return `[${text}]`;
	};
	const status = new module.StatusIndicator("working", ui, (text: string) => `<${text}>`, colorMessage, raw);
	const snapshots: string[][] = [];
	for (const width of [1, 4, 12]) {
		snapshots.push(status.render(width));
		status.setMessage(`${raw}updated`);
		snapshots.push(status.render(width));
	}
	status.dispose();
	return [snapshots, callbacks, requests];
}

async function captureSummaries(source: string, units: number[]): Promise<unknown> {
	const themeModule = await import(pathToFileURL(path.join(source, "modes/interactive/theme/theme.ts")).href);
	const branchModule = await import(pathToFileURL(path.join(source, "modes/interactive/components/branch-summary-message.ts")).href);
	const skillModule = await import(pathToFileURL(path.join(source, "modes/interactive/components/skill-invocation-message.ts")).href);
	const identity = (text: string): string => text;
	const theme = Object.fromEntries([
		"heading", "link", "linkUrl", "code", "codeBlock", "codeBlockBorder", "quote", "quoteBorder",
		"hr", "listBullet", "bold", "italic", "strikethrough", "underline",
	].map(key => [key, identity]));
	themeModule.setThemeInstance(new themeModule.Theme(
		{ customMessageLabel: 0, customMessageText: 0, dim: 0 }, { customMessageBg: 0 }, "256color",
	));
	const plain = (lines: string[]): string[] => lines.map(text => text.replace(/\x1b\[[0-9;]*m/g, ""));
	const raw = String.fromCharCode(...units);
	const branch = new branchModule.BranchSummaryMessageComponent(
		{ role: "branchSummary", summary: raw, fromId: "root", timestamp: 0 }, theme,
	);
	const skill = new skillModule.SkillInvocationMessageComponent({ name: raw, location: "path", content: raw }, theme);
	const snapshots = [];
	for (const width of [4, 12, 40]) {
		for (const expanded of [false, true]) {
			branch.setExpanded(expanded);
			skill.setExpanded(expanded);
			snapshots.push([plain(branch.render(width)), plain(skill.render(width))]);
			branch.invalidate();
			skill.invalidate();
			snapshots.push([plain(branch.render(width)), plain(skill.render(width))]);
		}
	}
	return snapshots;
}

async function captureDialogs(source: string, units: number[]): Promise<unknown> {
	const themeModule = await import(pathToFileURL(path.join(source, "modes/interactive/theme/theme.ts")).href);
	const selectorModule = await import(pathToFileURL(path.join(source, "modes/interactive/components/extension-selector.ts")).href);
	const inputModule = await import(pathToFileURL(path.join(source, "modes/interactive/components/extension-input.ts")).href);
	themeModule.setThemeInstance(new themeModule.Theme(
		{ accent: 0, text: 0, muted: 0, dim: 0, border: 0 }, {}, "256color",
	));
	const raw = String.fromCharCode(...units);
	const callbacks: string[] = [];
	const selector = new selectorModule.ExtensionSelectorComponent(raw, [raw, `${raw}next`], (value: string) => callbacks.push(value), () => {});
	const input = new inputModule.ExtensionInputComponent(raw, undefined, (value: string) => callbacks.push(value), () => {});
	input.handleInput(raw);
	const plain = (lines: string[]): string[] => lines.map(text => text.replace(/\x1b\[[0-9;]*m/g, ""));
	const snapshots = [];
	for (const width of [4, 12, 40]) {
		snapshots.push([plain(selector.render(width)), plain(input.render(width))]);
		selector.invalidate();
		input.invalidate();
		snapshots.push([plain(selector.render(width)), plain(input.render(width))]);
	}
	selector.handleInput("j");
	selector.handleInput("\r");
	input.handleInput("\r");
	selector.dispose();
	input.dispose();
	return [snapshots, callbacks];
}

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const module = await import(pathToFileURL(path.join(source, "core/keybindings.ts")).href);
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line);
			let output: unknown;
			if (request.operation === "dialogs") {
				output = await captureDialogs(source, request.units);
			} else if (request.operation === "summaries") {
				output = await captureSummaries(source, request.units);
			} else if (request.operation === "status") {
				output = await captureStatus(source, request.units);
			} else {
				const manager = module.KeybindingsManager.create(request.directory);
				output = [manager.getKeys("app.interrupt"), manager.getUserBindings()];
			}
			process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify(output) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
		}
	}
}
void main();
