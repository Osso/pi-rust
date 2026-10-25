import { writeSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { ModelSelectorComponent } = await import(
	pathToFileURL(`${root}/modes/interactive/components/model-selector.ts`).href
);
const { ModelRegistry } = await import(
	pathToFileURL(`${root}/core/model-registry.ts`).href
);
const { AuthStorage } = await import(
	pathToFileURL(`${root}/core/auth-storage.ts`).href
);
const themes = await import(
	pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href
);
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { kind: string; colors: string[] };
	themes.setThemeInstance(
		new themes.Theme(
			Object.fromEntries(request.colors.map((key) => [key, 0])),
			{},
			"256color",
		),
	);
	let constructed = false;
	let renders = 0;
	process.on("unhandledRejection", (reason) => {
		const rejection =
			reason instanceof Error
				? {
						kind: "error",
						value: {
							name: reason.name,
							message: reason.message,
							...Object.fromEntries(
								Object.entries(reason).filter(([key]) => key !== "stack"),
							),
						},
					}
				: { kind: "value", value: reason };
		writeSync(
			1,
			`PROBE:${JSON.stringify({ unhandled: true, constructed, renders, rejection })}\n`,
		);
		process.exit(23);
	});
	const value = "selector-\uD800";
	const thrown =
		request.kind === "error"
			? Object.assign(new TypeError(value), {
					code: "E_SELECTOR",
					detail: { marker: "original" },
				})
			: value;
	const storage = AuthStorage.inMemory();
	const registry = ModelRegistry.inMemory(storage);
	registry.registerProvider("selector-rejection", {
		api: "openai-completions",
		baseUrl: "https://fixture.invalid",
		models: [
			{
				id: "one",
				name: "One",
				reasoning: false,
				input: ["text"],
				cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
				contextWindow: 128,
				maxTokens: 32,
			},
		],
		oauth: {
			name: "Selector Fixture",
			login: async () => {
				throw new Error("unused login");
			},
			refreshToken: async (credential: unknown) => credential,
			getApiKey: (credential: { access: string }) => credential.access,
			modifyModels: () => {
				throw thrown;
			},
		},
	});
	storage.set("selector-rejection", {
		type: "oauth",
		access: "access",
		refresh: "refresh",
		expires: 9999999999999,
	});
	new ModelSelectorComponent(
		{ requestRender: () => renders++ },
		undefined,
		registry,
		[],
		() => {},
		() => {},
	);
	constructed = true;
	setTimeout(() => {
		writeSync(
			1,
			`PROBE:${JSON.stringify({ unhandled: false, constructed, renders })}\n`,
		);
		process.exit(0);
	}, 100);
}
