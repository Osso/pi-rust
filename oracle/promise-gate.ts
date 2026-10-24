export function createPromiseGate(): { promise: Promise<void>; resolve: () => void } {
	let resolve: (() => void) | undefined;
	const promise = new Promise<void>(finish => { resolve = finish; });
	if (resolve === undefined) throw new Error("Promise gate executor did not run synchronously");
	return { promise, resolve };
}
