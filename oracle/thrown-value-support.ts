export type ThrownValueSpec =
	| { kind: "value"; value: unknown }
	| { kind: "dom"; name: string; message: string }
	| { kind: "error"; constructor: "Error" | "TypeError"; message: string; name?: string; properties?: Record<string, unknown>; cause?: ThrownValueSpec };

export function materializeThrownValue(spec: ThrownValueSpec): unknown {
	if (spec.kind === "value") return spec.value;
	if (spec.kind === "dom") return new DOMException(spec.message, spec.name);
	const constructor = spec.constructor === "TypeError" ? TypeError : Error;
	const error = new constructor(spec.message, spec.cause === undefined ? undefined : { cause: materializeThrownValue(spec.cause) });
	if (spec.name !== undefined) error.name = spec.name;
	Object.assign(error, spec.properties ?? {});
	return error;
}

export function snapshotThrownValue(value: unknown): unknown {
	if (!(value instanceof Error)) return value;
	return {
		name: value.name, message: value.message,
		...(value instanceof DOMException ? { code: value.code } : {}),
		...Object.fromEntries(Object.getOwnPropertyNames(value)
			.filter((key) => !["stack", "name", "message"].includes(key))
			.map((key) => [key, snapshotThrownValue((value as unknown as Record<string, unknown>)[key])])),
	};
}
