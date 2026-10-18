// Oracle for `utils/version-check.ts`: semver comparison and error formatting (no network).
import { serve } from "./config-auth-oracle-support.ts";

type VersionCheckApi = {
	comparePackageVersions(left: string, right: string): number | undefined;
	isNewerPackageVersion(candidate: string, current: string): boolean;
	formatVersionCheckError(error: unknown): string;
};

function buildError(spec: {
	message?: string;
	causes?: Array<{ message?: string; code?: string }>;
	aggregate?: boolean;
}): unknown {
	const causes = (spec.causes ?? []).map((cause) => {
		const error = new Error(cause.message ?? "");
		if (cause.code !== undefined) (error as { code?: string }).code = cause.code;
		return error;
	});
	const cause =
		spec.aggregate === true ? new AggregateError(causes) : causes.length === 1 ? causes[0] : undefined;
	return new Error(spec.message ?? "", cause === undefined ? undefined : { cause });
}

async function main(): Promise<void> {
	const { pathToFileURL } = await import("node:url");
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("version check oracle requires a source module path");
	const api = (await import(pathToFileURL(sourcePath).href)) as VersionCheckApi;

	serve((request) => {
		if (request.op === "compare") {
			const comparison = api.comparePackageVersions(request.left as string, request.right as string);
			return { comparison: comparison === undefined ? null : Math.sign(comparison) };
		}
		if (request.op === "newer") {
			return {
				newer: api.isNewerPackageVersion(request.candidate as string, request.current as string),
			};
		}
		return {
			message: api.formatVersionCheckError(buildError(request.error as Parameters<typeof buildError>[0])),
		};
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`version check oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
