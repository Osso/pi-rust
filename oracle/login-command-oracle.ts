import { readFileSync } from "node:fs";
import { Readable, Writable } from "node:stream";
import { pathToFileURL } from "node:url";
import { runOracle, type OracleRequest } from "./coding-agent-utils-oracle-support.ts";

type Callbacks = {
	onAuth(info: { url: string; instructions?: string }): void;
	onDeviceCode(info: { verificationUri: string; userCode: string; expiresInSeconds: number }): void;
	onProgress(text: string): void;
	onPrompt(info: { message: string; placeholder: string }): Promise<string>;
	onManualCodeInput(): Promise<string>;
	onSelect(info: { message: string; options: { id: string; label: string }[] }): Promise<string | undefined>;
};
type Storage = { getOAuthProviders(): { id: string; name: string }[] };
type Registry = { registerOAuthProvider(provider: Record<string, unknown>): void; unregisterOAuthProvider(id: string): void };
type LoginApi = { handleLoginCommand(args: string[], dependencies: Record<string, unknown>): Promise<boolean> };

function registerFixtureProvider(oauth: Registry, request: OracleRequest): void {
	oauth.registerOAuthProvider({
		id: "lane-login", name: "Lane Login",
		login: async (callbacks: Callbacks) => {
			callbacks.onAuth({ url: "https://example.invalid/login", instructions: "Open browser" });
			if (request.flow === "browser") return { refresh: "refresh-token", access: "browser-access", expires: 4000000000000 };
			callbacks.onDeviceCode({ verificationUri: "https://example.invalid/device", userCode: "CODE-1234", expiresInSeconds: 60 });
			callbacks.onProgress("Ready");
			const answer = await callbacks.onPrompt({ message: "Token", placeholder: "hint" });
			const manual = await callbacks.onManualCodeInput();
			const options = request.emptyOptions ? [] : [{ id: "first", label: "First" }, { id: "second", label: "Second" }];
			const selected = await callbacks.onSelect({ message: "Account", options });
			if (request.fail) throw new Error("provider rejected");
			return { refresh: "refresh-token", access: `${answer}|${manual}|${selected ?? "none"}`, expires: 4000000000000 };
		},
		refreshToken: async () => { throw new Error("unused refresh"); },
		getApiKey: (credentials: { access: string }) => credentials.access,
	});
}

function captureWrites(write: (value: string) => void): Writable {
	return new Writable({ decodeStrings: false, write(chunk: string, _encoding, callback) { write(chunk); callback(); } });
}

async function captureLogin(api: LoginApi, storage: Storage, request: OracleRequest): Promise<unknown> {
	let stdout = "";
	let stderr = "";
	const opener = request.opener === "default" ? undefined : async () => {
		if (request.opener === "failure") throw new Error("open rejected");
	};
	process.exitCode = undefined;
	const handled = await api.handleLoginCommand(request.args as string[], {
		authStorage: storage, input: Readable.from([String(request.input ?? "")]),
		output: captureWrites((value) => { stdout += value; }), error: captureWrites((value) => { stderr += value; }),
		openBrowserCliUrl: opener,
	});
	return {
		handled, exitCode: process.exitCode ?? null, stdout, stderr,
		file: readFileSync(String(request.authPath), "utf8"),
		providers: storage.getOAuthProviders().map(({ id, name }) => ({ id, name })),
	};
}

void runOracle(async (module, request) => {
	const source = pathToFileURL(process.argv[2]);
	const storageApi = await import(new URL("../core/auth-storage.ts", source).href);
	const oauth = await import(new URL("../../../ai/src/utils/oauth/index.ts", source).href) as Registry;
	if (!request.withoutFixture) registerFixtureProvider(oauth, request);
	try {
		return await captureLogin(module as unknown as LoginApi, storageApi.AuthStorage.create(String(request.authPath)), request);
	} finally {
		process.exitCode = 0;
		oauth.unregisterOAuthProvider("lane-login");
	}
}).catch((error: unknown) => {
	console.error("login-command oracle failed:", error);
	process.exitCode = 1;
});
