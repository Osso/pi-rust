// Actual frozen semver comparison and newer predicate.
import { importSource, serve } from "./config-auth-oracle-support.ts";

type VersionCheckApi = {
	comparePackageVersions(left: string, right: string): number | undefined;
	isNewerPackageVersion(candidate: string, current: string): boolean;
};

async function main(): Promise<void> {
	const api = await importSource<VersionCheckApi>();
	serve((request) => {
		if (request.op === "compare") {
			const comparison = api.comparePackageVersions(request.left as string, request.right as string);
			return { comparison: comparison === undefined ? null : Math.sign(comparison) };
		}
		if (request.op === "newer") {
			return { newer: api.isNewerPackageVersion(request.candidate as string, request.current as string) };
		}
		throw new Error(`Unknown version operation: ${String(request.op)}`);
	});
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
