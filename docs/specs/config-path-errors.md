# Configuration path errors

- Package/README and default XDG/home path failures remain catchable at the same boundaries as frozen config.ts.
- Error carriers retain the originating native error, including constructor identity, code, SystemError info, errno/syscall and URL input properties. Message projection does not replace the source object.
- PathError→ConfigError preserves the native source. File-URL validation retains TypeError/URIError and Node error codes.
- Preserve source lookup ordering and source catches. No alternate getter, panic-based compatibility path, or catch_unwind around path resolution.
- [Traceability](../porting/traceability/config.md) distinguishes the carrier prerequisite from the getter/consumer Result cascade.
