import { serve } from "./config-auth-oracle-support.ts";

serve((request) => (request.sources as number[][]).map((units) => {
	try {
		JSON.parse(String.fromCharCode(...units));
		throw new Error("fixture must be invalid JSON");
	} catch (error) {
		if (!(error instanceof SyntaxError)) throw error;
		return {
			name: error.name,
			message: Array.from({ length: error.message.length }, (_, i) => error.message.charCodeAt(i)),
			display: Buffer.from(error.message, "utf8").toString("utf8"),
		};
	}
}));
