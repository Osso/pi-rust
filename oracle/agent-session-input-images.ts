import { dirname, resolve } from "node:path";
import { fauxAssistantMessage } from "@earendil-works/pi-ai/compat";

export async function runInputImagesScenario(context, continueInput) {
	const {
		session,
		sessionManager,
		settingsManager,
		modelRegistry,
		modules,
		dir,
		core,
	} = context;
	const root = resolve(dirname(process.argv[2]), "../../..");
	const { loadExtensionFromFactory } = await import(
		`${root}/coding-agent/src/core/extensions/loader.ts`
	);
	const { createEventBus } = await import(
		`${root}/coding-agent/src/core/event-bus.ts`
	);
	const views = [];
	const runtime = modules.createExtensionRuntime();
	const extension = await loadExtensionFromFactory(
		(api) => {
			api.on("input", (event) => {
				if (continueInput)
					return event.text === "clear"
						? { action: "transform", text: event.text, images: [] }
						: { action: "continue" };
				views.push(event.images === undefined ? "absent" : event.images);
				return { action: "handled" };
			});
			api.on("before_agent_start", (event) => {
				if (continueInput)
					views.push(event.images === undefined ? "absent" : event.images);
			});
		},
		dir,
		createEventBus(),
		runtime,
	);
	session._extensionRunner = new modules.ExtensionRunner(
		[extension],
		runtime,
		dir,
		sessionManager,
		modelRegistry,
		settingsManager,
	);
	session.setActiveToolsByName([]);
	core.setResponses(
		Array.from({ length: 4 }, () => fauxAssistantMessage("reply")),
	);
	const image = { type: "image", data: "AA==", mimeType: "image/png" };
	try {
		await session.prompt("absent");
		await session.prompt("empty", { images: [] });
		await session.prompt(continueInput ? "clear" : "image", {
			images: [image],
		});
		if (!continueInput) return views;
		await session.prompt("image", { images: [image] });
		const users = session.messages
			.filter((message) => message.role === "user")
			.map((message) => message.content);
		return { views, users };
	} finally {
		session.dispose();
	}
}
