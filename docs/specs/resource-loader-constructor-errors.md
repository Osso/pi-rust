# Resource-loader constructor errors

Match frozen `resource-loader.ts:302–311` at the [common pin](../porting/source-lock.json).

- Constructor path resolution must throw its original error, never silently retain the unresolved input.
- Settings/default-package-manager construction errors retain the original callback/native value. Do not replace them with message strings or fresh generic Errors.
- SDK, session-services, supervisor and project-trust caller adapters preserve that carrier until an explicit source catch/display boundary.
- In particular, HOME-unset NSS errors retain SystemError branding, message, code, errno, syscall and info fields. The shared [homedir authority's approved failure gap](../porting/traceability/node-os.md) is unchanged.

`resource_loader_constructor_errors` executes actual frozen/native constructor, SDK, session-services and default project-trust loader boundaries. A private LD_PRELOAD NSS interposer arms only after module/dependency initialization and permits preceding caller path lookups before failing at constructor entry. No host NSS state changes or provider/network calls.

The constructor slice does not revalidate other resource-loader helpers or reload diagnostics. [Branch/proof map](../porting/traceability/resource-loader.md#constructor-error-carrier).
