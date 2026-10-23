import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { ModelSelectorComponent } = await import(
	pathToFileURL(`${root}/modes/interactive/components/model-selector.ts`).href
);
const { ModelRegistry } = await import(pathToFileURL(`${root}/core/model-registry.ts`).href);
const { AuthStorage } = await import(pathToFileURL(`${root}/core/auth-storage.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { path: string; colors: string[] };
	const colors = Object.fromEntries(request.colors.map((key) => [key, 0]));
	themes.setThemeInstance(new themes.Theme(colors, {}, "256color"));
	const registry = new ModelRegistry(AuthStorage.inMemory(), request.path);
	for (const key of ["HOME", "GOOGLE_APPLICATION_CREDENTIALS", "GOOGLE_CLOUD_API_KEY"]) {
		delete process.env[key];
	}
	process.env.PI_TEST_FAIL_PASSWD = "1";
	let renders = 0;
	const selector = new ModelSelectorComponent(
		{ requestRender: () => renders++ },
		undefined,
		registry,
		[],
		() => {},
		() => {},
	);
	await new Promise((resolve) => setImmediate(resolve));
	console.log(JSON.stringify({ renders, rows: selector.render(160) }));
}
