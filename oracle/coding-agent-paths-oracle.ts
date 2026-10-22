import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Options = {
	trim?: boolean;
	expandTilde?: boolean;
	homeDir?: string;
	stripAtPrefix?: boolean;
	normalizeUnicodeSpaces?: boolean;
};

type Case =
	| { kind: "is_local"; name: string; input: string }
	| { kind: "normalize"; name: string; input: string; options?: Options }
	| { kind: "resolve"; name: string; input: string; baseDir: string; options?: Options }
	| { kind: "relative"; name: string; filePath: string; cwd: string }
	| { kind: "format_relative"; name: string; filePath: string; cwd: string }
	| { kind: "canonicalize"; name: string; path: string };


void runOracle((api, request) => {
	const isLocalPath = api.isLocalPath as (value: string) => boolean;
	const normalizePath = api.normalizePath as (value: string, options?: Options) => string;
	const resolvePath = api.resolvePath as (value: string, baseDir?: string, options?: Options) => string;
	const getCwdRelativePath = api.getCwdRelativePath as (filePath: string, cwd: string) => string | undefined;
	const formatPathRelativeToCwdOrAbsolute = api.formatPathRelativeToCwdOrAbsolute as (
		filePath: string,
		cwd: string,
	) => string;
	const canonicalizePath = api.canonicalizePath as (value: string) => string;

	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			try {
				switch (input.kind) {
					case "is_local":
						return { name: input.name, ok: true, value: isLocalPath(input.input) };
					case "normalize":
						return { name: input.name, ok: true, value: normalizePath(input.input, input.options) };
					case "resolve":
						return {
							name: input.name,
							ok: true,
							value: resolvePath(input.input, input.baseDir, input.options),
						};
					case "relative":
						return { name: input.name, ok: true, value: getCwdRelativePath(input.filePath, input.cwd) ?? null };
					case "format_relative":
						return {
							name: input.name,
							ok: true,
							value: formatPathRelativeToCwdOrAbsolute(input.filePath, input.cwd),
						};
					case "canonicalize":
						return { name: input.name, ok: true, value: canonicalizePath(input.path) };
				}
			} catch {
				return { name: input.name, ok: false };
			}
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`paths oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
