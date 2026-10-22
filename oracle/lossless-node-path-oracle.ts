import * as path from "node:path";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { kind: string; input: string; base: string };

void runOracle((_api, request) => ({
    results: requireCases(request).map((value) => {
        const item = value as Case;
        switch (item.kind) {
            case "normalize": return path.posix.normalize(item.input);
            case "join": return path.posix.join(item.base, item.input);
            case "resolve": return path.posix.resolve(item.base, item.input);
            case "relative": return path.posix.relative(item.base, item.input);
            case "dirname": return path.posix.dirname(item.input);
            case "basename": return path.posix.basename(item.input);
            case "absolute": return path.posix.isAbsolute(item.input);
            default: throw new Error(`Unknown path operation: ${item.kind}`);
        }
    }),
})).catch((error: unknown) => {
    process.stderr.write(`${String(error)}\n`);
    process.exitCode = 1;
});
