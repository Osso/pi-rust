import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { createInterface } from "node:readline";
import { runInNewContext } from "node:vm";

const source = process.argv[2];
const packageRoot = source.slice(0, source.indexOf("/packages/"));
const ts = createRequire(resolve(packageRoot, "package.json"))("typescript");
const contents = readFileSync(source, "utf8");
const ast = ts.createSourceFile(source, contents, ts.ScriptTarget.Latest, true);
const declarations = ast.statements.filter((statement: import("typescript").Statement) => {
	if (ts.isFunctionDeclaration(statement)) return statement.name?.text === "formatFollowUpPreview";
	return ts.isVariableStatement(statement) && statement.declarationList.declarations.some(
		(declaration: import("typescript").VariableDeclaration) =>
			ts.isIdentifier(declaration.name) && declaration.name.text === "FOLLOW_UP_BODY_PREVIEW_LENGTH",
	);
});
if (declarations.length !== 2) throw new Error("Frozen followup helper declarations missing");
const javascript = ts.transpileModule(
	declarations.map((statement: import("typescript").Statement) => statement.getText(ast)).join("\n") +
		"\nglobalThis.preview = formatFollowUpPreview;",
	{ compilerOptions: { target: ts.ScriptTarget.ES2022 } },
).outputText;
const context: { preview?: (message: string) => string } = {};
runInNewContext(javascript, context);
if (!context.preview) throw new Error("Frozen followup helper did not initialize");
const preview = context.preview;
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { message: string };
	process.stdout.write(JSON.stringify(preview(request.message)) + "\n");
}
