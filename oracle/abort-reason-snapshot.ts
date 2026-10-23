// Observational snapshots of actual reasons; omit only Error stacks by user approval.
function errorRecord(error: Error): Record<string, unknown> {
	const result: Record<string, unknown> = { name: error.name, message: error.message };
	const properties = error as unknown as Record<string, unknown>;
	for (const key of ["code", "errno", "syscall", "path", "cause", ...Object.keys(error)]) {
		if (key === "stack" || key === "name" || key === "message" || properties[key] === undefined) continue;
		const value = properties[key];
		result[key] = value instanceof Error ? errorRecord(value) : value;
	}
	return result;
}

export function describeAbortReason(reason: unknown): unknown {
	if (reason === undefined) return { kind: "undefined" };
	if (reason instanceof Error) return { kind: reason instanceof DOMException ? "dom" : "error", value: errorRecord(reason) };
	if (typeof reason === "number") return { kind: "number", value: Object.is(reason, -0) ? "-0" : String(reason) };
	return { kind: "value", value: reason };
}

