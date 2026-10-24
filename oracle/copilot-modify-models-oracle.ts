import { importSource, serve } from "./config-auth-oracle-support.ts";

type Source = {
 githubCopilotOAuthProvider: { modifyModels(models: unknown[], credentials: unknown): unknown[] };
};
async function main(): Promise<void> {
 const { githubCopilotOAuthProvider } = await importSource<Source>();
 serve(request => githubCopilotOAuthProvider.modifyModels(request.models as unknown[], request.credentials));
}
void main();
