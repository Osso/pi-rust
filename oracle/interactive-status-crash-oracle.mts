import { pathToFileURL } from "node:url";

const root = process.argv[2];
const code = process.env.PI_STATUS_CRASH_CODE;
const kind = process.env.PI_STATUS_CRASH_KIND;
if (!root || !code || !kind)
	throw new Error("source root and private crash case required");
const { InteractiveMode } = await import(
	pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href
);
const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
	isShuttingDown: true,
	signalCleanupHandlers: [],
	ui: {
		stop() {
			throw new Error("already-shutting-down crash must not stop the terminal");
		},
	},
});
const error =
	kind === "raw"
		? { code, message: "status policy" }
		: Object.assign(new TypeError("status policy"), { code });
// Real frozen :337-342 and :5206-5223, including the emergency-before-guard order.
InteractiveMode.prototype.uncaughtCrash.call(receiver, error);
