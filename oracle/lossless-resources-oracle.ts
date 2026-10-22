import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

async function main() {
    const api = await import(pathToFileURL(process.argv[2]).href);
    for await (const line of createInterface({ input: process.stdin })) {
        const request = JSON.parse(line);
        try {
            if (request.operation === "prepareSkill") {
                mkdirSync(request.dir, { recursive: true });
                writeFileSync(join(request.dir, "SKILL.md"), "---\nname: example\ndescription: sample skill\n---\nbody\n");
            }
            const output = request.operation === "prepareSkill"
                ? api.loadSkillsFromDir({ dir: request.dir, source: "path" })
                : request.operation === "frontmatter"
                ? api.parseFrontmatter(request.content)
                : api.substituteArgs(request.content, request.args);
            console.log(JSON.stringify({ id: request.id, ok: true, output }));
        } catch (error) {
            console.log(JSON.stringify({ id: request.id, ok: false, error: String(error) }));
        }
    }
}
main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});
