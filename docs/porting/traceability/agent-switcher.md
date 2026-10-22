
## Q18 native UTF-16

Render/input, labels, IDs and selection callbacks use `JsString`; preview normalization and ID truncation operate on UTF-16 units. Tests in `raw_error_consumer_components` written **unexecuted**, library compiler-blocked. No internal Node UTF-8 encoding.
