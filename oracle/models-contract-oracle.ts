import { importSource, serve } from "./config-auth-oracle-support.ts";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { normalizeAssistantMessageClock } from "./assistant-snapshots.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

type Model = { id: string };
type Provider = {
	id: string;
	getModels(): Model[];
	refreshModels?: () => Promise<void>;
};
type Stream = { result(): Promise<Record<string, unknown>> };
type Models = {
	setProvider(provider: Provider): void;
	deleteProvider(id: string): void;
	clearProviders(): void;
	getProviders(): Provider[];
	getModels(provider?: string): Model[];
	getModel(provider: string, id: string): Model | undefined;
	refresh(provider?: string): Promise<void>;
	getAuth(model: Model): Promise<unknown>;
	stream(model: Model, context: unknown, options?: unknown): Stream;
	complete(
		model: Model,
		context: unknown,
		options?: unknown,
	): Promise<Record<string, unknown>>;
	streamSimple(model: Model, context: unknown, options?: unknown): Stream;
	completeSimple(
		model: Model,
		context: unknown,
		options?: unknown,
	): Promise<Record<string, unknown>>;
};
type StreamCase = {
	name: string;
	model: Model;
	auth?: unknown;
	options?: Record<string, unknown>;
	missingProvider?: boolean;
	missingApi?: boolean;
	rejection?: unknown;
};
type Source = {
	createModels(): Models;
	createProvider(input: Record<string, unknown>): Provider;
	ModelsError: new (
		code: string,
		message: string,
		options?: { cause: unknown },
	) => Error;
};
const ids = (models: Model[]) => models.map((model) => model.id);

async function main(): Promise<void> {
	const api = await importSource<Source>();
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("models source path required");
	const eventStream = await import(
		pathToFileURL(join(dirname(sourcePath), "utils/event-stream.ts")).href
	);
	const input = (
		id: string,
		models: Model[],
		refreshModels?: () => Promise<Model[]>,
	) => ({
		id,
		models,
		refreshModels,
		auth: { apiKey: { name: "Ambient", resolve: async () => ({ auth: {} }) } },
		api: {
			stream() {
				throw new Error("must not stream");
			},
			streamSimple() {
				throw new Error("must not stream");
			},
		},
	});
	serve(async (request) => {
		const initial = request.initial as Model[];
		const refreshed = request.refreshed as Model[];
		const models = api.createModels();
		if (request.operation === "streams") {
			const results = [];
			for (const testCase of request.cases as StreamCase[]) {
				const models = api.createModels(),
					captured: unknown[] = [];
				const respond =
					(method: string) =>
					(model: unknown, _context: unknown, options: unknown) => {
						const requestOptions = (options ?? {}) as Record<string, unknown>;
						captured.push({
							method,
							model,
							serializedModel: JSON.stringify(model),
							apiKey: requestOptions.apiKey ?? null,
							headers: requestOptions.headers ?? null,
							env: requestOptions.env ?? null,
							temperature: requestOptions.temperature ?? null,
						});
						const stream = new eventStream.AssistantMessageEventStream();
						stream.push({
							type: "done",
							reason: "stop",
							message: request.response,
						});
						stream.end(request.response);
						return stream;
					};
				if (!testCase.missingProvider)
					models.setProvider(
						api.createProvider({
							id: "dynamic",
							name: "metadata name",
							baseUrl: "http://metadata.invalid",
							headers: { "metadata-only": "not injected" },
							auth: {
								apiKey: {
									name: "Fixture",
									resolve: async () => {
										if ("rejection" in testCase) throw testCase.rejection;
										return testCase.auth;
									},
								},
							},
							models: [testCase.model],
							api: testCase.missingApi
								? {}
								: {
										stream: respond("stream"),
										streamSimple: respond("simple"),
									},
						}),
					);
				let auth;
				try {
					auth = {
						ok: true,
						value: (await models.getAuth(testCase.model)) ?? null,
					};
				} catch (error) {
					auth = { ok: false, error: describeAbortReason(error) };
				}
				const context = { messages: [] },
					messages = [];
				messages.push(
					await models
						.stream(testCase.model, context, testCase.options)
						.result(),
				);
				messages.push(
					await models.complete(testCase.model, context, testCase.options),
				);
				messages.push(
					await models
						.streamSimple(testCase.model, context, testCase.options)
						.result(),
				);
				messages.push(
					await models.completeSimple(
						testCase.model,
						context,
						testCase.options,
					),
				);
				results.push({
					name: testCase.name,
					auth,
					captured,
					messages: messages.map(normalizeAssistantMessageClock),
				});
			}
			return results;
		}
		if (request.operation === "collection") {
			models.setProvider(api.createProvider(input("first", initial)));
			models.setProvider(api.createProvider(input("second", refreshed)));
			models.setProvider(api.createProvider(input("first", refreshed)));
			const broken = api.createProvider(input("broken", []));
			broken.getModels = () => {
				throw new Error("catalog broken");
			};
			models.setProvider(broken);
			const before = {
				providers: models.getProviders().map((provider) => provider.id),
				models: ids(models.getModels()),
				first: ids(models.getModels("first")),
				broken: ids(models.getModels("broken")),
				missing: ids(models.getModels("missing")),
				found: models.getModel("first", "new")?.id ?? null,
				absent: models.getModel("first", "absent") ?? null,
			};
			models.deleteProvider("first");
			const deleted = models.getProviders().map((provider) => provider.id);
			models.clearProviders();
			return {
				before,
				deleted,
				cleared: models.getProviders().map((provider) => provider.id),
			};
		}
		if (request.operation === "wrapped_error") {
			const inner = new api.ModelsError("oauth", "inner error", {
				cause: false,
			});
			const outer = new Error("outer error", { cause: inner });
			models.setProvider(
				api.createProvider(
					input("dynamic", initial, async () => {
						throw outer;
					}),
				),
			);
			try {
				await models.refresh("dynamic");
			} catch (error) {
				return describeAbortReason(error);
			}
			throw new Error("fixture refresh must fail");
		}
		if (request.operation === "refresh_errors") {
			let calls = 0;
			models.setProvider(api.createProvider(input("static", initial)));
			models.setProvider(
				api.createProvider(
					input("bad", initial, async () => {
						calls++;
						throw request.cause;
					}),
				),
			);
			const existing = new api.ModelsError("oauth", "existing error", {
				cause: request.cause,
			});
			models.setProvider(
				api.createProvider(
					input("existing", initial, async () => {
						calls++;
						throw existing;
					}),
				),
			);
			const errors = [];
			for (const id of ["bad", "existing"]) {
				try {
					await models.refresh(id);
				} catch (error) {
					errors.push(describeAbortReason(error));
				}
			}
			await models.refresh("missing");
			await models.refresh("static");
			await models.refresh();
			return { calls, errors, models: ids(models.getModels("bad")) };
		}
		if (request.operation === "inflight") {
			let calls = 0;
			let settle:
				| { resolve(models: Model[]): void; reject(reason: unknown): void }
				| undefined;
			const pending = () => {
				if (!settle)
					throw new Error("refresh must expose its pending settlement");
				return settle;
			};
			const provider = api.createProvider(
				input("dynamic", initial, () => {
					calls++;
					return new Promise<Model[]>((resolve, reject) => {
						settle = { resolve, reject };
					});
				}),
			);
			const refresh = provider.refreshModels;
			if (!refresh)
				throw new Error("dynamic provider must expose refreshModels");
			const first = refresh(),
				second = refresh();
			const before = { calls, models: ids(provider.getModels()) };
			pending().resolve(refreshed);
			await Promise.all([first, second]);
			const after = { calls, models: ids(provider.getModels()) };
			const failed = refresh();
			pending().reject(request.cause);
			let failure;
			try {
				await failed;
			} catch (error) {
				failure = describeAbortReason(error);
			}
			const retained = { calls, models: ids(provider.getModels()) };
			const retried = refresh();
			pending().resolve(initial);
			await retried;
			return {
				before,
				after,
				failure,
				retained,
				retried: { calls, models: ids(provider.getModels()) },
			};
		}
		if (request.operation === "concurrent_all") {
			const starts: string[] = [],
				releases: (() => void)[] = [];
			for (const id of ["one", "two"]) {
				let values: Model[] = [];
				const provider = api.createProvider(input(id, []));
				provider.getModels = () => values;
				provider.refreshModels = () => {
					starts.push(id);
					return new Promise<void>((resolve, reject) =>
						releases.push(() => {
							if (id === "one") {
								values = initial;
								resolve();
							} else reject(request.cause);
						}),
					);
				};
				models.setProvider(provider);
			}
			const refresh = models.refresh();
			const started = [...starts];
			for (const release of releases) release();
			await refresh;
			return {
				started,
				one: ids(models.getModels("one")),
				two: ids(models.getModels("two")),
			};
		}
		models.setProvider(
			api.createProvider(input("dynamic", initial, async () => refreshed)),
		);
		const before = ids(models.getModels("dynamic"));
		await models.refresh("dynamic");
		return { before, after: ids(models.getModels("dynamic")) };
	});
}
void main();
