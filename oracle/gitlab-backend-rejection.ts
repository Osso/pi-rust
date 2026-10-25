// Values thrown by the import-boundary backend fixture, not by a source patch.
export function throwBackendRejection(request: Record<string, unknown>): void {
	if (!request.backendThrow) return;
	const units = request.valueUnits;
	if (
		!Array.isArray(units) ||
		!units.every(
			(unit) => Number.isInteger(unit) && unit >= 0 && unit <= 0xffff,
		)
	) {
		throw new TypeError("Fixture valueUnits must contain UTF-16 units");
	}
	const message = String.fromCharCode(...units);
	if (request.backendThrow === "error") {
		throw Object.assign(new TypeError(message), { code: "E_BACKEND" });
	}
	if (request.backendThrow === "value") throw message;
	if (request.backendThrow === "conversion")
		throw { toString: null, valueOf: null };
	throw new Error("Unknown fixture backendThrow kind");
}
