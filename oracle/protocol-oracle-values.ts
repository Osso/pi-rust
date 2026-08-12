export type ClientHelloRequest = {
	type: "hello";
	version: number;
};

export type CommandRequest = {
	command: string;
	cwd?: string;
	name?: string;
	model?: {
		provider: string;
		id: string;
	};
	thinkingLevel?: string;
	sessionId?: string;
	text?: string;
};

export type TaggedNumber =
	| {
			number: "finite";
			value: number;
		}
	| {
			number: "negative_zero" | "nan" | "positive_infinity" | "negative_infinity";
		};

export function decodeHex(hex: string): Uint8Array {
	const bytes = new Uint8Array(hex.length / 2);
	for (let index = 0; index < bytes.length; index += 1) {
		bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
	}
	return bytes;
}

export function encodeHex(bytes: Uint8Array): string {
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

export function decodeTaggedNumber(number: TaggedNumber): number {
	switch (number.number) {
		case "finite":
			return number.value;
		case "negative_zero":
			return -0;
		case "nan":
			return Number.NaN;
		case "positive_infinity":
			return Number.POSITIVE_INFINITY;
		case "negative_infinity":
			return Number.NEGATIVE_INFINITY;
	}
}

export function float64Bits(value: number): string {
	const buffer = new ArrayBuffer(8);
	const view = new DataView(buffer);
	view.setFloat64(0, value, false);
	return view.getBigUint64(0, false).toString(16).padStart(16, "0");
}
