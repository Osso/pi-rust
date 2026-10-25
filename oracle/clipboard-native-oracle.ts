import { runOracle } from "./coding-agent-utils-oracle-support.ts";

void runOracle((api, request) => {
	const availableBefore = api.clipboard !== null;
	process.env.TERMUX_VERSION = String(request.nextTermux);
	process.env.DISPLAY = String(request.nextDisplay);
	process.env.WAYLAND_DISPLAY = "";
	return [availableBefore, api.clipboard !== null];
}).catch((error: unknown) => {
	console.error("clipboard-native oracle failed:", error);
	process.exitCode = 1;
});
