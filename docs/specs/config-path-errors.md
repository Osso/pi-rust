# Configuration path errors

- Package/README and default XDG/home path failures remain catchable at the same boundaries as frozen config.ts.
- Error carriers retain the originating native error, including constructor identity, code, SystemError info, errno/syscall and URL input properties. Message projection does not replace the source object.
- PathError→ConfigError preserves the native source. File-URL validation retains TypeError/URIError and Node error codes.
- Preserve source lookup ordering and source catches. No alternate getter, panic-based compatibility path, or catch_unwind around path resolution.
- [Traceability](../porting/traceability/config.md) distinguishes the carrier prerequisite from the getter/consumer Result cascade.

## Metadata and derived paths

- Package name, version, app name/title and config-directory name preserve exact UTF-16 from native JSON parsing, including unmatched units. Package file bytes use Node UTF-8 decoding; syntax errors retain their lossless messages.
- Original configuration text/path getters retain UTF-16 and use Node path joining. Encode only when crossing filesystem, process or terminal interfaces; do not add alternate scalar getters.
- Environment names remain native strings. Derive them with JavaScript uppercasing and encode at the environment boundary; human help labels derive separately from raw metadata and pad before terminal encoding.
- HTTP metadata headers use the shared WebIDL ByteString boundary, not UTF-8 replacement. See [lossless content](lossless-content.md).
