// Synthetic public auth returns and overlapping in-memory refreshes, against the locked module.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const sourceUrl = pathToFileURL(process.argv[2]);
	const api = await import(sourceUrl.href);
	const oauth = await import(new URL("../../../ai/src/utils/oauth/index.ts", sourceUrl).href);
	serve(async (raw) => {
		const { op } = raw as { op: string };
		const id = "auth-refresh-boundary";
		const reads: string[] = [];
		let release!: () => void;
		const overlap = new Promise<void>((resolve) => { release = resolve; });
		oauth.registerOAuthProvider({
			id, name: "Synthetic",
			login: async () => { throw new Error("unused"); },
			refreshToken: async (credential: { refresh: string }) => {
				reads.push(credential.refresh);
				if (reads.length === 2) release();
				await overlap;
				return { refresh: "B", access: "new", expires: 4_000_000_000_000 };
			},
			getApiKey: (credential: { access: string }) => credential.access,
		});
		try {
			const credential = op === "surrogate"
				? { type: "oauth", refresh: "A", access: "\ud800", expires: 4_000_000_000_000 }
				: { type: "oauth", refresh: "A", access: "old", expires: 0 };
			const storage = api.AuthStorage.inMemory({ [id]: credential });
			if (op === "surrogate") {
				const result = await storage.getApiKey(id);
				return { resultJson: JSON.stringify(result), units: Array.from({ length: result.length }, (_, i) => result.charCodeAt(i)) };
			}
			const results = await Promise.all([storage.getApiKey(id), storage.getApiKey(id)]);
			return { reads, results, credential: storage.get(id) };
		} finally {
			oauth.unregisterOAuthProvider(id);
		}
	});
}
void main().catch(error => { process.stderr.write(`${String(error)}\n`); process.exitCode = 1; });
