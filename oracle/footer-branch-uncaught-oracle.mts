import { writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

process.once("uncaughtException", (error: Error) => {
	const value = {
		name: error.name,
		message: error.message,
		code: Reflect.get(error, "code"),
	};
	process.stdout.write(`${JSON.stringify(value)}\n`, () => process.exit(1));
});
const { FooterDataProvider } = (await import(
	pathToFileURL(process.argv[2]).href
)) as {
	FooterDataProvider: new (
		cwd: string,
	) => {
		getGitBranch(): string | null;
		onBranchChange(callback: () => void): () => void;
	};
};
const cwd = process.env.PI_FOOTER_CHILD_CWD;
if (!cwd) throw new Error("private cwd required");
const provider = new FooterDataProvider(cwd);
provider.getGitBranch();
provider.onBranchChange(() => {
	throw Object.assign(new TypeError("branch-\ud800"), { code: "EBRANCH" });
});
setTimeout(() => process.exit(7), 3000);
await writeFile(`${cwd}/.git/HEAD`, "ref: refs/heads/next\n");
