import { importSource, serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

type Source = { ModelsError: new (code: string, message: string, options: { cause: unknown }) => Error };
async function main(): Promise<void> {
 const { ModelsError } = await importSource<Source>();
 serve(request => {
  const cause = request.kind === "native"
   ? Object.assign(new TypeError("native detail", { cause: request.cause }), { code: "E_FIXTURE" })
   : request.cause;
  return describeAbortReason(new ModelsError("model_source", "refresh failed", { cause }));
 });
}
void main();
