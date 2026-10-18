// Generates crates/pi-tui/src/unicode_tables.rs.
//
// Sources:
// - https://www.unicode.org/Public/17.0.0/emoji/emoji-sequences.txt
// - https://www.unicode.org/Public/17.0.0/emoji/emoji-zwj-sequences.txt
// - get-east-asian-width 1.6.0 lookup-data.js (pinned by packages/tui/package.json)
// - Character properties evaluated with the running Node ICU (Node 26.11.1, ICU 78.3, Unicode 17.0)
//
// Usage: node crates/pi-tui/scripts/generate-unicode-tables.mjs <emoji-sequences.txt> <emoji-zwj-sequences.txt> <get-east-asian-width-dir>

import { readFile, writeFile } from "node:fs/promises";
import * as path from "node:path";
import { pathToFileURL } from "node:url";

const [sequencesPath, zwjPath, eastAsianWidthDir] = process.argv.slice(2);
if (!sequencesPath || !zwjPath || !eastAsianWidthDir) {
	throw new Error("expected emoji-sequences.txt, emoji-zwj-sequences.txt, and get-east-asian-width directory");
}

const propertyRegexes = {
	zeroWidth: /^(?:\p{Default_Ignorable_Code_Point}|\p{Control}|\p{Mark}|\p{Surrogate})+$/v,
	leadingNonPrinting: /^[\p{Default_Ignorable_Code_Point}\p{Control}\p{Format}\p{Mark}\p{Surrogate}]+/v,
	nonPrintingChar: /^(?:\p{Default_Ignorable_Code_Point}|\p{Control}|\p{Format}|\p{Mark}|\p{Surrogate})$/v,
	mark: /^\p{Mark}$/v,
	spacingMark: /^\p{Spacing_Mark}$/v,
	cjkBreak:
		/[\p{Script_Extensions=Han}\p{Script_Extensions=Hiragana}\p{Script_Extensions=Katakana}\p{Script_Extensions=Hangul}\p{Script_Extensions=Bopomofo}]/u,
};

const RGI = /^\p{RGI_Emoji}$/v;

function collectRanges(test) {
	const ranges = [];
	let start;
	for (let codePoint = 0; codePoint <= 0x10ffff; codePoint++) {
		if (codePoint >= 0xd800 && codePoint <= 0xdfff) continue;
		const matches = test(String.fromCodePoint(codePoint));
		if (matches && start === undefined) start = codePoint;
		if (!matches && start !== undefined) {
			ranges.push([start, codePoint - 1]);
			start = undefined;
		}
	}
	if (start !== undefined) ranges.push([start, 0x10ffff]);
	return ranges;
}

function parseCodepointField(field) {
	const codepoints = field.trim();
	if (codepoints.includes("..")) {
		const [first, last] = codepoints.split("..");
		return { range: [Number.parseInt(first, 16), Number.parseInt(last, 16)] };
	}
	return { sequence: codepoints.split(/\s+/).map((value) => Number.parseInt(value, 16)) };
}

async function readRgiSequences(filename) {
	const text = await readFile(filename, "utf8");
	const sequences = [];
	for (const line of text.split("\n")) {
		const body = line.split("#")[0].trim();
		if (!body) continue;
		const [codepoints, type] = body.split(";").map((part) => part.trim());
		if (!type?.includes("Emoji")) continue;
		const parsed = parseCodepointField(codepoints);
		if (parsed.range) {
			for (let codePoint = parsed.range[0]; codePoint <= parsed.range[1]; codePoint++) {
				sequences.push(String.fromCodePoint(codePoint));
			}
		} else {
			sequences.push(String.fromCodePoint(...parsed.sequence));
		}
	}
	return sequences;
}

function compareSequences(left, right) {
	const leftPoints = Array.from(left, (char) => char.codePointAt(0));
	const rightPoints = Array.from(right, (char) => char.codePointAt(0));
	for (let index = 0; index < Math.min(leftPoints.length, rightPoints.length); index++) {
		if (leftPoints[index] !== rightPoints[index]) return leftPoints[index] - rightPoints[index];
	}
	return leftPoints.length - rightPoints.length;
}

function rustString(value) {
	let result = '"';
	for (const char of value) {
		const codePoint = char.codePointAt(0);
		if (char === '"') result += '\\"';
		else if (char === "\\") result += "\\\\";
		else if (codePoint >= 0x20 && codePoint <= 0x7e) result += char;
		else result += `\\u{${codePoint.toString(16)}}`;
	}
	return `${result}"`;
}

function rustRanges(name, ranges) {
	const pairs = Array.isArray(ranges[0]) ? ranges : ranges.reduce((result, value, index, all) => {
		if (index % 2 === 0) result.push([value, all[index + 1]]);
		return result;
	}, []);
	const entries = pairs.map(([start, end]) => `    (0x${start.toString(16)}, 0x${end.toString(16)}),`).join("\n");
	return `pub(crate) static ${name}: &[(u32, u32)] = &[\n${entries}\n];\n`;
}

const sequences = [...(await readRgiSequences(sequencesPath)), ...(await readRgiSequences(zwjPath))];
const unique = [...new Set(sequences)];
for (const sequence of unique) {
	if (!RGI.test(sequence)) throw new Error(`Unicode data lists non-RGI sequence ${JSON.stringify(sequence)}`);
}
unique.sort(compareSequences);

const eastAsianWidth = await import(pathToFileURL(path.join(eastAsianWidthDir, "lookup-data.js")).href);
const zeroWidth = collectRanges((value) => propertyRegexes.zeroWidth.test(value));
const leadingNonPrinting = collectRanges((value) => propertyRegexes.leadingNonPrinting.test(value));
const nonPrintingChar = collectRanges((value) => propertyRegexes.nonPrintingChar.test(value));
const mark = collectRanges((value) => propertyRegexes.mark.test(value));
const spacingMark = collectRanges((value) => propertyRegexes.spacingMark.test(value));
const cjkBreak = collectRanges((value) => propertyRegexes.cjkBreak.test(value));

const header = `// Generated by crates/pi-tui/scripts/generate-unicode-tables.mjs. Do not edit.
//
// Unicode 17.0.0 emoji data:
//   https://www.unicode.org/Public/17.0.0/emoji/emoji-sequences.txt
//   https://www.unicode.org/Public/17.0.0/emoji/emoji-zwj-sequences.txt
// East Asian Width ranges from get-east-asian-width 1.6.0 (pinned by packages/tui/package.json).
// Character property ranges evaluated with Node 26.11.1 (ICU 78.3, Unicode 17.0).

`;

const rgiEntries = unique.map((sequence) => `    ${rustString(sequence)},`).join("\n");

const body = `${rustRanges("ZERO_WIDTH_RANGES", zeroWidth)}
${rustRanges("LEADING_NON_PRINTING_RANGES", leadingNonPrinting)}
${rustRanges("NON_PRINTING_RANGES", nonPrintingChar)}
${rustRanges("MARK_RANGES", mark)}
${rustRanges("SPACING_MARK_RANGES", spacingMark)}
${rustRanges("CJK_BREAK_RANGES", cjkBreak)}
${rustRanges("EAW_WIDE_RANGES", eastAsianWidth.wideRanges)}
${rustRanges("EAW_FULLWIDTH_RANGES", eastAsianWidth.fullwidthRanges)}
pub(crate) static RGI_EMOJI: &[&str] = &[
${rgiEntries}
];

fn in_ranges(ranges: &[(u32, u32)], code_point: u32) -> bool {
    ranges
        .binary_search_by(|&(start, end)| {
            if end < code_point {
                std::cmp::Ordering::Less
            } else if start > code_point {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

pub(crate) fn is_zero_width(value: char) -> bool {
    in_ranges(ZERO_WIDTH_RANGES, value as u32)
}

pub(crate) fn is_leading_non_printing(value: char) -> bool {
    in_ranges(LEADING_NON_PRINTING_RANGES, value as u32)
}

pub(crate) fn is_non_printing(value: char) -> bool {
    in_ranges(NON_PRINTING_RANGES, value as u32)
}

pub(crate) fn is_mark(value: char) -> bool {
    in_ranges(MARK_RANGES, value as u32)
}

pub(crate) fn is_spacing_mark(value: char) -> bool {
    in_ranges(SPACING_MARK_RANGES, value as u32)
}

pub(crate) fn is_cjk_break(value: char) -> bool {
    in_ranges(CJK_BREAK_RANGES, value as u32)
}

pub(crate) fn east_asian_width(value: char) -> usize {
    let code_point = value as u32;
    if in_ranges(EAW_FULLWIDTH_RANGES, code_point) || in_ranges(EAW_WIDE_RANGES, code_point) {
        2
    } else {
        1
    }
}

pub(crate) fn is_rgi_emoji(value: &str) -> bool {
    RGI_EMOJI.binary_search(&value).is_ok()
}
`;

const output = path.join(import.meta.dirname, "..", "src", "unicode_tables.rs");
await writeFile(output, header + body);
process.stderr.write(`wrote ${output} with ${unique.length} RGI emoji sequences\n`);
