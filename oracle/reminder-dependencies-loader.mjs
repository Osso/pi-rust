const reference = "file:///home/osso/.worktrees/pi-rust-source-4f9e6f8a4/packages/coding-agent/src/core/session-control-db.ts";

export async function resolve(specifier, context, nextResolve) {
  try {
    return await nextResolve(specifier, context);
  } catch (error) {
    if (error.code !== "ERR_MODULE_NOT_FOUND" || specifier.startsWith(".") || specifier.startsWith("/") || specifier.startsWith("node:")) throw error;
    return nextResolve(specifier, { ...context, parentURL: reference });
  }
}
