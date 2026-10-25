import { createInterface } from "node:readline";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const { DefaultResourceLoader } = await import(pathToFileURL(process.argv[2]).href);
	const { SettingsManager } = await import(pathToFileURL(join(dirname(process.argv[2]), "settings-manager.ts")).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const skillDir = join(request.root, "skill");
		mkdirSync(skillDir, { recursive: true });
		const writeSkill = (name: string) => writeFileSync(join(skillDir, "SKILL.md"), `---\nname: ${name}\ndescription: fixture\n---\n${name} body\n`);
		writeSkill("first");
		const seen: string[][] = [];
		const loader = new DefaultResourceLoader({ cwd: request.root, agentDir: request.root, settingsManager: SettingsManager.inMemory(), noExtensions: true, noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true, additionalSkillPaths: [skillDir], promptsOverride: (base: unknown) => { seen.push(loader.getSkills().skills.map((skill: { name: string }) => skill.name)); return base; } });
		await loader.reload();
		writeSkill("second");
		mkdirSync(join(request.root, "rules", "bad.md"), { recursive: true });
		let error: string | null = null;
		try { await loader.reload(); } catch (failure) { error = failure instanceof Error ? failure.message : String(failure); }
		process.stdout.write(`${JSON.stringify({ id: request.id, seen, skills: loader.getSkills().skills.map((skill: { name: string }) => skill.name), error })}\n`);
	}
}
void main().catch((error: unknown) => { console.error(error); process.exit(1); });
