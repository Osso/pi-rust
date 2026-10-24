import { importSource, serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

type Source = {
	registerOAuthProvider(provider: unknown): void;
	unregisterOAuthProvider(id: string): void;
	getOAuthApiKey(id: string, credentials: unknown): Promise<unknown>;
};

function abortWithFixtureReason(request: Record<string, unknown>): AbortSignal {
	const controller = new AbortController();
	if (request.default) {
		controller.abort();
	} else if (request.nativeMessage !== undefined) {
		const reason = new TypeError(request.nativeMessage as string, { cause: false });
		controller.abort(Object.assign(reason, { code: "E_CANCEL" }));
	} else {
		controller.abort(request.reason);
	}
	return controller.signal;
}

function registerFixtureProvider(api: Source, signal: AbortSignal): void {
	api.registerOAuthProvider({
		id: "abort-fixture",
		name: "Fixture",
		login: async () => { throw new Error("must not login"); },
		refreshToken: async () => { throw signal.reason; },
		getApiKey: () => "unused",
	});
}

async function main(): Promise<void> {
	const api = await importSource<Source>();
	serve(async request => {
		registerFixtureProvider(api, abortWithFixtureReason(request));
		try {
			await api.getOAuthApiKey("abort-fixture", {
				"abort-fixture": { refresh: "r", access: "old", expires: 0 },
			});
			throw new Error("fixture refresh must reject");
		} catch (error) {
			return describeAbortReason(error);
		} finally {
			api.unregisterOAuthProvider("abort-fixture");
		}
	});
}

void main();
