# Windows OCR word-pack boundary

`ocr-word-pack.json` is the production Windows Agent's source-controlled OCR policy boundary. The
file is embedded into the executable with `include_str!`; the Agent does not accept a runtime path
or environment override for these rules. Changing a rule therefore requires source review,
building, signing, and distributing a new Agent binary.

Startup strictly parses, bounds, validates, and compiles the embedded document before any screen
capture can begin. Every monitor then receives a separately compiled `WordPack`. Invalid JSON,
unknown fields, empty or excessive rule sets, unsupported categories, inconsistent rule kinds and
risks, overlong fields, and invalid regular expressions fail closed. Diagnostics expose only a
stable configuration error code and never rule patterns or recognized text.

The initial policy covers the `explicit_term`, `adult_service`, and `medical_education` categories
with English, Simplified Chinese, and Traditional Chinese literal, regular-expression, and
exemption rules.
