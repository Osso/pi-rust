// Constructor identity is independent of mutable Error.name and message fields.
import { serve } from "./config-auth-oracle-support.ts";
const constructors = { Error, EvalError, RangeError, ReferenceError, SyntaxError, TypeError, URIError, AggregateError };
serve((input) => {
	const constructor = constructors[input.constructor as keyof typeof constructors];
	const error = constructor === AggregateError ? new AggregateError([], input.message as string) : new constructor(input.message as string);
	if (input.name !== undefined) error.name = input.name as string;
	return {
		value:{name:error.name,message:error.message},
		instances:Object.fromEntries(Object.entries(constructors).map(([name,type])=>[name,error instanceof type])),
	};
});
