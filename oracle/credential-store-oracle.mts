import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import type {
	Credential,
	CredentialStore,
} from "@pi-oracle-source/ai/src/auth/types.ts";

type StoreModule =
	typeof import("@pi-oracle-source/ai/src/auth/credential-store.ts");

function latch() {
	let resolve!: () => void;
	const promise = new Promise<void>((done) => {
		resolve = done;
	});
	return { promise, resolve };
}

function credential(key: string): Credential {
	return { type: "api_key", key };
}

async function queueScenario(store: CredentialStore) {
	const entered = latch();
	const gate = latch();
	await store.modify("p", async () => credential("seed"));
	const first = store.modify("p", async () => {
		entered.resolve();
		await gate.promise;
		return credential("first");
	});
	await entered.promise;
	let secondInput: Credential | undefined;
	const second = store.modify("p", async (current) => {
		secondInput = current;
		return credential("second");
	});
	const deletion = store.delete("p");
	const during = await store.read("p");
	const other = await store.modify("q", async () => credential("other"));
	gate.resolve();
	const results = [await first, await second];
	await deletion;
	return {
		during,
		other,
		results,
		secondInput,
		final: (await store.read("p")) ?? null,
	};
}

async function recoveryScenario(store: CredentialStore) {
	const entered = latch();
	const gate = latch();
	await store.modify("p", async () => credential("seed"));
	const failure = store
		.modify("p", async () => {
			entered.resolve();
			await gate.promise;
			throw new Error("denied");
		})
		.then(
			() => false,
			() => true,
		);
	await entered.promise;
	const following = store.modify("p", async () => undefined);
	gate.resolve();
	const rejected = await failure;
	const unchanged = await following;
	const preserved = await store.read("p");
	const missingNoop = (await store.modify("q", async () => undefined)) ?? null;
	await store.delete("p");
	const deleted = (await store.read("p")) ?? null;
	const recreated = await store.modify("p", async () =>
		credential("recovered"),
	);
	return { rejected, unchanged, preserved, missingNoop, deleted, recreated };
}

async function main() {
	const source = process.argv[2];
	if (!source)
		throw new Error("credential store oracle requires a source module");
	const api = (await import(pathToFileURL(source).href)) as StoreModule;
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line) as {
			id: number;
			scenario: "queue" | "recovery";
		};
		const store = new api.InMemoryCredentialStore();
		const output =
			request.scenario === "queue"
				? await queueScenario(store)
				: await recoveryScenario(store);
		process.stdout.write(
			`${JSON.stringify({ id: request.id, ok: true, output })}\n`,
		);
	}
}

void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
