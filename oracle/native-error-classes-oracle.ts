// Constructor identity is independent of mutable Error.name and message fields.
import { serve } from "./config-auth-oracle-support.ts";
import { materializeThrownValue, snapshotThrownValue, type ThrownValueSpec } from "./thrown-value-support.ts";
const constructors = { Error, EvalError, RangeError, ReferenceError, SyntaxError, TypeError, URIError, AggregateError };
serve((input) => {
	if (input.operation === "cause") {
		const cause = materializeThrownValue(input.cause as ThrownValueSpec);
		const error = new Error("outer");
		Object.assign(error, { before: 1, cause, after: 2 });
		return {
			value: snapshotThrownValue(error),
			causeError: cause instanceof Error,
			causeDom: cause instanceof DOMException,
			causeString: String(cause),
		};
	}
	const constructor = constructors[input.constructor as keyof typeof constructors];
	const error = constructor === AggregateError ? new AggregateError([], input.message as string) : new constructor(input.message as string);
	if (input.name !== undefined) error.name = input.name as string;
	return {
		value:{name:error.name,message:error.message},
		instances:Object.fromEntries(Object.entries(constructors).map(([name,type])=>[name,error instanceof type])),
	};
});
