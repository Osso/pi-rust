import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

interface CopilotModule {
	loginGitHubCopilot(options: {
		onDeviceCode: () => void;
		onPrompt: () => Promise<string>;
	}): Promise<unknown>;
}

async function capturePolicyConcurrency(api: CopilotModule) {
	const originalFetch = globalThis.fetch;
	let active = 0;
	let maximum = 0;
	let count = 0;
	globalThis.fetch = async (input) => {
		const url = String(input);
		if (url.endsWith("/login/device/code")) {
			return Response.json({
				device_code: "device-code",
				user_code: "ABCD-EFGH",
				verification_uri: "https://github.com/login/device",
				interval: 1,
				expires_in: 900,
			});
		}
		if (url.endsWith("/login/oauth/access_token"))
			return Response.json({ access_token: "ghu_refresh_token" });
		if (url.includes("/copilot_internal/v2/token")) {
			return Response.json({ token: "plain-token", expires_at: 9999999999 });
		}
		if (url.endsWith("/models")) return Response.json({ data: [] });
		if (url.includes("/models/") && url.endsWith("/policy")) {
			count += 1;
			active += 1;
			maximum = Math.max(maximum, active);
			await new Promise((resolve) => setTimeout(resolve, 10));
			active -= 1;
			return Response.json({});
		}
		throw new Error(`Unexpected fetch URL: ${url}`);
	};
	try {
		await api.loginGitHubCopilot({
			onDeviceCode: () => {},
			onPrompt: async () => "",
		});
		return { count, maximum };
	} finally {
		globalThis.fetch = originalFetch;
	}
}

const source = process.argv[2];
if (!source)
	throw new Error("Copilot policy oracle requires the frozen module path");
const api = (await import(pathToFileURL(source).href)) as CopilotModule;
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { id: number };
	process.stdout.write(
		`${JSON.stringify({ id: request.id, ok: true, output: await capturePolicyConcurrency(api) })}\n`,
	);
}
