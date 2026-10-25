# Disable broadcast native extension

The native port is registered hidden as `<native:disable-broadcast>` and loads only when selected by the user's extension path.

On every `session_start`, read the current active tool list and set it to the same ordered list with all exact `broadcast` names removed. Do not change tools at extension load time or remove other names.
