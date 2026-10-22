// Recorded provider fixtures only: never delegate to the host's network fetch.
let activeScript: typeof globalThis.fetch | undefined;

export function installScriptedFetch(): void {
	globalThis.fetch = (input, init) => {
		if (!activeScript)
			return Promise.reject(new Error("fetch outside scripted oracle request"));
		return activeScript(input, init);
	};
}

export function setFetchScript(script: typeof globalThis.fetch): void {
	activeScript = script;
}

export function clearFetchScript(): void {
	activeScript = undefined;
}
