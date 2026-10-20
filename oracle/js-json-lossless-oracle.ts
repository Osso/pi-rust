// No normalization: compare JSON.parse/stringify output bytes and errors verbatim.
import { serve } from "./config-auth-oracle-support.ts";

serve((request) => (request.texts as string[]).map((text) => {
	try {
		const value = JSON.parse(text);
		return {
			compact: JSON.stringify(value),
			pretty: JSON.stringify(value, null, 2),
			keys: value !== null && typeof value === "object" && !Array.isArray(value)
				? Object.keys(value).map((key) => Array.from({ length: key.length }, (_, i) => key.charCodeAt(i)))
				: null,
		};
	} catch (error) {
		return { error: (error as Error).message };
	}
}));
