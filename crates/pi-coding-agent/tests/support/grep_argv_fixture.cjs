const fs = require("node:fs");
const fixture = JSON.parse(process.env.PI_FAKE_RG_ARGV);
if (process.argv[2] === "--version") {
	if (fixture.kind === "vanished") fs.unlinkSync(__filename);
	if (fixture.kind === "permission") fs.chmodSync(__filename, 0o644);
	process.exit(0);
}
fs.writeFileSync(fixture.argvFile, JSON.stringify(process.argv.slice(2)));
if (fixture.kind === "signal") process.kill(process.pid, "SIGTERM");
else fs.writeSync(1, JSON.stringify({ type: "match", data: { path: { text: fixture.filePath }, line_number: 2, lines: { text: "inline\n" } } }) + "\n");
