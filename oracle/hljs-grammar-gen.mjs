/**
 * Serializes highlight.js 10.7.3 language definitions into the JSON grammar IR
 * interpreted by the Rust port. Modes are emitted as an id table so shared and
 * cyclic mode graphs survive serialization. Regenerate with:
 *
 *   node oracle/hljs-grammar-gen.mjs <highlight.js package dir> <output.json>
 */
import { createRequire } from "node:module";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [, , packageDir, outputPath] = process.argv;
if (!packageDir || !outputPath) {
	throw new Error("usage: hljs-grammar-gen.mjs <highlight.js package dir> <output.json>");
}

const require = createRequire(join(packageDir, "package.json"));
const hljs = require("./lib/core");

const shebang = hljs.SHEBANG();
const endSameAsBegin = hljs.END_SAME_AS_BEGIN({});
const knownFunctions = new Map([
	[String(shebang["on:begin"]), "shebang"],
	[String(endSameAsBegin["on:begin"]), "end_same_begin"],
	[String(endSameAsBegin["on:end"]), "end_same_end"],
]);

// Fields hljs computes during compilation; they are not part of the definition.
const COMPILED_FIELDS = new Set([
	"isCompiled",
	"cachedVariants",
	"__beforeBegin",
	"parent",
	"keywordPatternRe",
	"matcher",
	"beginRe",
	"endRe",
	"illegalRe",
	"terminatorEnd",
	"exports",
]);

const indexSource = readFileSync(join(packageDir, "lib/index.js"), "utf8");
const languages = [...indexSource.matchAll(/registerLanguage\('([^']+)', require\('\.\/languages\/([^']+)'\)\)/g)].map(
	([, name, file]) => ({ name, file }),
);

class Serializer {
	constructor() {
		this.defs = {};
		this.ids = new WeakMap();
		this.nextId = 0;
	}

	ref(value) {
		const existing = this.ids.get(value);
		if (existing !== undefined) return existing;
		const id = this.nextId++;
		this.ids.set(value, id);
		const definition = { id };
		this.defs[id] = definition;
		for (const key of Object.keys(value)) {
			if (COMPILED_FIELDS.has(key)) continue;
			// `begin: false` and friends are falsy in the source; omit them instead.
			if (value[key] === false && ["begin", "end", "illegal", "lexemes", "match"].includes(key)) continue;
			// Keyword tables and class aliases are plain data, not mode references.
			if (key === "keywords" || key === "classNameAliases") {
				definition[key] = this.plain(value[key]);
				continue;
			}
			definition[key] = this.value(value[key]);
		}
		return id;
	}

	plain(value) {
		if (value === null || value === undefined) return null;
		if (value instanceof RegExp) return { $regex: value.source };
		if (Array.isArray(value)) return value.map((entry) => this.plain(entry));
		if (typeof value === "object") {
			const result = {};
			for (const key of Object.keys(value)) result[key] = this.plain(value[key]);
			return result;
		}
		return value;
	}

	value(value) {
		if (value === null || value === undefined) return null;
		if (typeof value === "string" || typeof value === "boolean" || typeof value === "number") return value;
		if (value instanceof RegExp) {
			if (value.flags !== "") throw new Error(`unsupported regex flags ${value.flags}`);
			return { $regex: value.source };
		}
		if (Array.isArray(value)) return value.map((entry) => this.value(entry));
		if (typeof value === "function") {
			const source = String(value);
			const known = knownFunctions.get(source);
			if (!known && source.includes("afterMatchIndex") && source.includes("hasClosingTag")) {
				return { $fn: "jsx_truly_opening_tag" };
			}
			if (!known) throw new Error(`unsupported function: ${source.slice(0, 160)}`);
			return { $fn: known };
		}
		return { $ref: this.ref(value) };
	}
}

const output = { languages: [] };
const unsupported = [];
for (const { name, file } of languages) {
	const serializer = new Serializer();
	try {
		const definition = require(`./lib/languages/${file}`)(hljs);
		const root = serializer.ref(definition);
		output.languages.push({
			key: name,
			root,
			name: definition.name ?? null,
			aliases: definition.aliases ?? [],
			supersetOf: definition.supersetOf ?? null,
			disableAutodetect: definition.disableAutodetect ?? false,
			caseInsensitive: definition.case_insensitive ?? false,
			classNameAliases: definition.classNameAliases ?? {},
			defs: serializer.defs,
		});
	} catch (error) {
		unsupported.push({ name, reason: String(error.message ?? error) });
	}
}

writeFileSync(outputPath, `${JSON.stringify(output)}\n`);
process.stderr.write(`serialized ${output.languages.length} languages, ${unsupported.length} unsupported\n`);
for (const entry of unsupported) process.stderr.write(`  ${entry.name}: ${entry.reason}\n`);
