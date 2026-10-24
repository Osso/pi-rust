const fs = require("node:fs");
const path = require("node:path");
const { spawn } = require("node:child_process");
if (process.argv[2] === "--version") process.exit(0);
const fixture = JSON.parse(process.env.PI_FAKE_RG_CHILD);
fs.mkdirSync(fixture.directory, { recursive: true });
const file = name => path.join(fixture.directory, name);
const mark = name => fs.writeFileSync(file(name), "done");
const descendant = process.argv[2] === "--descendant";
process.on("SIGTERM", () => mark(descendant ? "descendant-sigterm" : "sigterm"));
if (!descendant) fs.writeFileSync(file("pid"), String(process.pid));
if (fixture.afterExit && !descendant) {
	spawn(process.execPath, [__filename, "--descendant", String(process.pid)], { stdio: ["ignore", "ignore", 2] }).unref();
	process.exit(0);
}
if (fixture.closed) { fs.closeSync(1); fs.closeSync(2); }
if (!descendant) mark("ready");
if (fixture.limit && !descendant) {
	fs.writeSync(1, JSON.stringify({ type: "match", data: { path: { text: "a.txt" }, line_number: 1, lines: { text: "inline\n" } } }) + "\n");
}
function parentExited() {
	try { process.kill(Number(process.argv[3]), 0); return false; }
	catch (error) { if (error.code !== "ESRCH") throw error; return true; }
}
setInterval(() => {
	if (descendant && parentExited()) mark("ready");
	if (fs.existsSync(file("finish"))) { mark("released"); process.exit(0); }
}, 5);
setTimeout(() => process.exit(88), 10000);
