import { createInterface } from "node:readline";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const { DefaultResourceLoader } = await import(
		pathToFileURL(process.argv[2]).href
	);
	const { SettingsManager } = await import(
		pathToFileURL(join(dirname(process.argv[2]), "settings-manager.ts")).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		if (request.operation === "theme_source") {
			const loader = new DefaultResourceLoader({
				cwd: request.root,
				agentDir: request.root,
				settingsManager: SettingsManager.inMemory(),
				noExtensions: true,
				noSkills: true,
				noPromptTemplates: true,
				noThemes: true,
				noContextFiles: true,
				additionalThemePaths: [request.theme],
			});
			await loader.reload();
			process.stdout.write(
				`${JSON.stringify({ id: request.id, themes: loader.getThemes().themes.map((theme: { sourceInfo?: unknown }) => theme.sourceInfo ?? null) })}\n`,
			);
			continue;
		}
		if (request.operation === "prompt_publication") {
			let revision = 0;
			const seen: Array<string | null> = [];
			const loader = new DefaultResourceLoader({
				cwd: request.root,
				agentDir: request.root,
				settingsManager: SettingsManager.inMemory(),
				noExtensions: true,
				noSkills: true,
				noPromptTemplates: true,
				noThemes: true,
				noContextFiles: true,
				systemPromptOverride: () => `system-${++revision}`,
				appendSystemPromptOverride: () => {
					seen.push(loader.getSystemPrompt() ?? null);
					return [`append-${revision}`];
				},
			});
			await loader.reload();
			await loader.reload();
			process.stdout.write(
				`${JSON.stringify({ id: request.id, seen, system: loader.getSystemPrompt(), append: loader.getAppendSystemPrompt() })}\n`,
			);
			continue;
		}
		if (request.operation === "warnings") {
			const warnings: string[] = [];
			const original = console.error;
			console.error = (message: string) => {
				warnings.push(message);
			};
			try {
				const loader = new DefaultResourceLoader({
					cwd: request.root,
					agentDir: request.root,
					settingsManager: SettingsManager.inMemory(),
					noExtensions: true,
					noSkills: true,
					noPromptTemplates: true,
					noThemes: true,
					systemPrompt: join(request.root, "prompt-dir"),
				});
				await loader.reload();
			} finally {
				console.error = original;
			}
			process.stdout.write(`${JSON.stringify({ id: request.id, warnings })}\n`);
			continue;
		}
		if (request.operation === "extend_path") {
			const loader = new DefaultResourceLoader({
				cwd: request.root,
				agentDir: request.root,
				settingsManager: SettingsManager.inMemory(),
			});
			let error: string | null = null;
			try {
				loader.extendResources({
					promptPaths: [
						{
							path: request.path,
							metadata: {
								source: "extension",
								scope: "temporary",
								origin: "top-level",
								baseDir: request.baseDir,
							},
						},
					],
				});
			} catch (failure) {
				error = failure instanceof Error ? failure.message : String(failure);
			}
			process.stdout.write(`${JSON.stringify({ id: request.id, error })}\n`);
			continue;
		}
		if (request.operation === "factory_errors") {
			const loader = new DefaultResourceLoader({
				cwd: request.root,
				agentDir: request.root,
				settingsManager: SettingsManager.inMemory(),
				noExtensions: true,
				noSkills: true,
				noPromptTemplates: true,
				noThemes: true,
				noContextFiles: true,
				extensionFactories: request.values.map((value: unknown) => () => {
					throw value;
				}),
			});
			await loader.reload();
			process.stdout.write(
				`${JSON.stringify({ id: request.id, errors: loader.getExtensions().errors })}\n`,
			);
			continue;
		}
		const skillDir = join(request.root, "skill");
		mkdirSync(skillDir, { recursive: true });
		const writeSkill = (name: string) =>
			writeFileSync(
				join(skillDir, "SKILL.md"),
				`---\nname: ${name}\ndescription: fixture\n---\n${name} body\n`,
			);
		writeSkill("first");
		const seen: string[][] = [];
		const loader = new DefaultResourceLoader({
			cwd: request.root,
			agentDir: request.root,
			settingsManager: SettingsManager.inMemory(),
			noExtensions: true,
			noSkills: true,
			noPromptTemplates: true,
			noThemes: true,
			noContextFiles: true,
			additionalSkillPaths: [skillDir],
			promptsOverride: (base: unknown) => {
				seen.push(
					loader
						.getSkills()
						.skills.map((skill: { name: string }) => skill.name),
				);
				return base;
			},
		});
		await loader.reload();
		writeSkill("second");
		mkdirSync(join(request.root, "rules", "bad.md"), { recursive: true });
		let error: string | null = null;
		try {
			await loader.reload();
		} catch (failure) {
			error = failure instanceof Error ? failure.message : String(failure);
		}
		writeFileSync(join(request.root, "extra.md"), "extra prompt");
		loader.extendResources({
			promptPaths: [
				{
					path: join(request.root, "extra.md"),
					metadata: {
						source: "extension",
						scope: "temporary",
						origin: "top-level",
					},
				},
			],
		});
		process.stdout.write(
			`${JSON.stringify({ id: request.id, seen, skills: loader.getSkills().skills.map((skill: { name: string }) => skill.name), error })}\n`,
		);
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exit(1);
});
