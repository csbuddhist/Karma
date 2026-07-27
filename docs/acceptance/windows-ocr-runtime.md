# Windows OCR runtime acceptance

Status: pending execution on physical Windows hosts. This document records the required matrix;
it does not claim production OCR accuracy or performance.

The temporary non-UI configuration boundary is:

- `KARMA_OCR_LIGHTWEIGHT_MANIFEST` (required for OCR): verified lightweight bundle manifest.
- `KARMA_OCR_ACCURATE_MANIFEST` (optional): verified accurate bundle manifest.
- `KARMA_OCR_PROFILE=auto|lightweight|accurate` (optional; defaults to `auto`). Invalid values
  leave frame/image inference available and report the stable `profile_invalid` OCR status.

Use only in-memory test content. Do not save screenshots, recognized text, OCR categories, model
paths, or URLs in this record.

| Platform | Displays | Profile | Content/scenario | P50 ms | P95 ms | CPU | Working set | Result | Notes |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| Windows 10 22H2 | 1 | lightweight | Simplified Chinese | | | | | pending | |
| Windows 10 22H2 | 2 | lightweight | Traditional Chinese | | | | | pending | |
| Windows 10 22H2 | 3 | lightweight | English and mixed text | | | | | pending | |
| Windows 10 22H2 | 1 | accurate | Video subtitles | | | | | pending | |
| Windows 10 22H2 | 2 | auto | Browser small fonts | | | | | pending | |
| Windows 10 22H2 | 3 | auto | Medical, education, news, code, game negatives | | | | | pending | |
| Windows 11 | 1 | lightweight | Simplified Chinese | | | | | pending | |
| Windows 11 | 2 | lightweight | Traditional Chinese | | | | | pending | |
| Windows 11 | 3 | lightweight | English and mixed text | | | | | pending | |
| Windows 11 | 1 | accurate | Video subtitles | | | | | pending | |
| Windows 11 | 2 | auto | Browser small fonts | | | | | pending | |
| Windows 11 | 3 | auto | Medical, education, news, code, game negatives | | | | | pending | |

For each platform also verify that image inference remains running when OCR bundle loading or
per-monitor OCR session creation fails, that each display has independent OCR availability, and
that no health output contains recognized text, categories, frame data, local paths, or URLs.
