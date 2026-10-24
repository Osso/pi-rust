// The literal union is the frozen exported data contract, read through TS's parser.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";

const path = process.argv[2];
const require = createRequire(join(dirname(path), "../package.json"));
const ts = require("typescript") as typeof import("typescript");
const source = ts.createSourceFile(
	path,
	readFileSync(path, "utf8"),
	ts.ScriptTarget.Latest,
	true,
);
const declaration = source.statements.find(
	(node): node is import("typescript").TypeAliasDeclaration =>
		ts.isTypeAliasDeclaration(node) && node.name.text === "KnownProvider",
);
if (!declaration || !ts.isUnionTypeNode(declaration.type))
	throw new Error("Frozen KnownProvider union is missing");
const providers = declaration.type.types.map((node) => {
	if (!ts.isLiteralTypeNode(node) || !ts.isStringLiteral(node.literal))
		throw new Error("KnownProvider member is not a literal");
	return node.literal.text;
});
for await (const _line of createInterface({ input: process.stdin })) {
	process.stdout.write(`${JSON.stringify(providers)}\n`);
}
