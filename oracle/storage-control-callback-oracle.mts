import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type ControlApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/session-control-db.ts");
const control: ControlApi = await import(pathToFileURL(process.argv[2]).href);
for await (const line of createInterface({ input: process.stdin })) {
	const request: {
		id: number;
		db: string;
		transaction: boolean;
		failure: unknown;
		typed: boolean;
	} = JSON.parse(line);
	control.withControlDb(request.db, (db) =>
		db.exec("CREATE TABLE callback_probe(value INTEGER)"),
	);
	let error: unknown;
	try {
		control.withControlDb(request.db, (db) => {
			const operation = () => {
				db.prepare("INSERT INTO callback_probe VALUES (1)").run();
				if (request.typed)
					throw Object.assign(new TypeError("callback failed"), {
						code: "FIXTURE",
					});
				throw request.failure;
			};
			return request.transaction
				? control.withImmediateTransaction(db, operation)
				: operation();
		});
	} catch (cause) {
		error =
			cause instanceof Error
				? {
						name: cause.name,
						message: cause.message,
						...Object.fromEntries(
							Object.entries(cause).filter(([key]) => key !== "stack"),
						),
					}
				: cause;
	}
	const count = control.withControlDb(request.db, (db) =>
		db.prepare("SELECT COUNT(*) AS count FROM callback_probe").get(),
	);
	console.log(JSON.stringify({ id: request.id, error, count }));
}
