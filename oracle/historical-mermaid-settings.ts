import { SettingsManager } from "/home/osso/.worktrees/pi-rust-source-532f7d12/packages/coding-agent/src/core/settings-manager.ts";

async function main() {
	const [project, agent, action] = process.argv.slice(2);
	const manager = SettingsManager.create(project, agent);
	if (action === "write-final") {
		manager.setMermaidRenderingMode("final");
		await manager.flush();
	}
	console.log(JSON.stringify({ mode: manager.getMermaidRenderingMode(), indent: manager.getCodeBlockIndent() }));
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
