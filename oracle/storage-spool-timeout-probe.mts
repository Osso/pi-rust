import { pathToFileURL } from "node:url";

const { withSeekableSessionTranscript } = await import(
	pathToFileURL(process.argv[2]).href
);
const started = performance.now();
try {
	withSeekableSessionTranscript(process.argv[3], () => {
		throw new Error("blocked FIFO must not reach callback");
	});
} catch (error) {
	console.log(
		JSON.stringify({
			elapsedMs: performance.now() - started,
			error: String(error),
		}),
	);
}
// Do not force exit: observe whether Worker.terminate releases a blocked open.
