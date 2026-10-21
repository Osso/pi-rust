import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// Selection is explicit; the mandatory runner also checks and read-only mounts every lock entry.
export function selectApprovedModule(frozenPath, module) {
	if (process.env.PI_APPROVED_DELTA !== "1") return frozenPath;
	const lock = JSON.parse(readFileSync(new URL("../docs/porting/delta-lock.json", import.meta.url), "utf8"));
	const entry = lock.deltas.find((delta) => Object.hasOwn(delta.modules, module));
	if (!entry) throw new Error(`No approved delta for ${module}`);
	const head = execFileSync("git", ["-C", entry.checkout, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
	if (head !== entry.commit) throw new Error(`Approved delta HEAD mismatch: ${head}`);
	const path = join(entry.checkout, module);
	const hash = createHash("sha256").update(readFileSync(path)).digest("hex");
	if (hash !== entry.modules[module]) throw new Error(`Approved delta module SHA256 mismatch: ${module}`);
	return path;
}
