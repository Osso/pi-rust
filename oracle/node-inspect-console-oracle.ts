import { inspect } from "node:util";
import { serve } from "./config-auth-oracle-support.ts";
serve((request) => ({ text: inspect(request.value) }));
