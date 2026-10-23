import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const units = (value: string): number[] =>
	Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
async function main(): Promise<void> {
	const source = process.argv[2]!;
	const { Image } = await import(
		pathToFileURL(`${source}/components/image.ts`).href
	);
	const terminal = await import(
		pathToFileURL(`${source}/terminal-image.ts`).href
	);
	const parse = (value: number | string | null): number | undefined =>
		value === null ? undefined : Number(value);
	function run(request: any): unknown {
		terminal.setCapabilities({
			images: request.protocol,
			trueColor: true,
			hyperlinks: false,
		});
		terminal.setCellDimensions({ widthPx: 9, heightPx: 18 });
		const seen: string[] = [];
		const image = new Image(
			"AAAA",
			"image/png",
			{
				fallbackColor: (value: string) => {
					seen.push(value);
					return `fallback:${value}`;
				},
			},
			{
				maxWidthCells: parse(request.maxWidth),
				maxHeightCells: parse(request.maxHeight),
				imageId: 42,
			},
			{ widthPx: 800, heightPx: 600 },
		);
		const snapshot = (width: number) => ({
			lines: image.render(width).map(units),
			imageId: image.getImageId(),
			fallback: seen.map(units),
		});
		const output = [snapshot(request.width), snapshot(request.width)];
		terminal.setCapabilities({
			images: null,
			trueColor: true,
			hyperlinks: false,
		});
		output.push(snapshot(request.width));
		image.invalidate();
		output.push(snapshot(request.width));
		output.push(snapshot(request.width + 1));
		return output;
	}
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			process.stdout.write(
				`${JSON.stringify({ ok: true, output: run(JSON.parse(line)) })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		}
	}
}
void main();
