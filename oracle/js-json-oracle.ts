// Oracle for `utils::js_json`: the engine's own JSON.parse / JSON.stringify.
// The source-module argument is unused; these are runtime built-ins.
import { serve } from "./config-auth-oracle-support.ts";

serve((request) => {
	const texts = request.texts as string[];
	return texts.map((text) => {
		try {
			const value = JSON.parse(text);
			return { compact: JSON.stringify(value), pretty: JSON.stringify(value, null, 2) };
		} catch (error) {
			// V8 can splice a surrogate pair in the context excerpt; Rust strings use U+FFFD there.
			return { error: (error as Error).message.toWellFormed() };
		}
	});
});
