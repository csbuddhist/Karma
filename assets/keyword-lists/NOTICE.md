# Window-title keyword data notice

`window-title-explicit.csv` is the editable source of the bundled window-title keyword list.
`tools/keyword-lists/build_title_keywords.py` converts it into `window-title-explicit.json`, which
the policy engine embeds and the console displays; do not edit the JSON by hand. The `sources`
column of the CSV records where each term came from:

- `LDNOOBW`: “Our List of Dirty, Naughty, Obscene, and Otherwise Bad Words,” © 2012–2020
  Shutterstock, Inc., licensed under Creative Commons Attribution 4.0 International
  (<https://creativecommons.org/licenses/by/4.0/>).
  Source: <https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words>
- `censor-text`, `Sensitive-lexicon`, `UT1`: third-party word lists merged into the CSV supplied
  for this list. Their exact upstream revisions and licenses must be confirmed, and any
  attribution or share-alike terms they carry recorded here, before a public release
  redistributes these terms.
- `Karma`: separately authored multi-word phrases kept from the earlier curated list, and the
  uncensored-series code prefixes `CCDV`, `CWP`, `DRC`, `HEYDOUGA`, `MCDV`, `RHJ`, and `SMD`.

The `code` category holds adult video product-code prefixes (番号) such as `SSIS`, which match
before three or four digits. They were collected from these sources, recorded in the same column:

- `MDCx`: <https://github.com/z291173301/MDCx> at commit
  `5e94c51553e258b70aa196415a4c2626ae3744a8`, licensed under GPL-3.0: the DMM series keys of
  `resources/userdata/dmm_cid_routes.json` (induced from libredmm), the series groups in
  `mdcx/crawlers/dmm_direct.py`, and the code prefixes of `resources/c_number/c_number.json`.
- `OpenAver`: <https://github.com/slive777/OpenAver> at commit
  `e334a560719e8021b285e0ddfb7118a1a64fd213`, `maker_mapping.json` and `dmm_prefix_table.json`,
  Copyright (c) 2026 peace, licensed under the MIT License.
- `darkeye`: <https://github.com/gty5678/darkeye> at commit
  `0de09749e34aa3350bf11d440c759d69d33b2179`, `resources/config/maker_prefix.json`, licensed under
  GPL-3.0.
- `webtor`: <https://github.com/webtor-io/web-ui> at commit
  `b46006f934a9114eef680765061bab692f223c22`, the JAV prefix alternation in
  `services/parse_torrent_name/main.go`, Copyright (c) 2022 webtor.io, licensed under the MIT
  License.
- `dhtsearch`: <https://github.com/madeye/dhtsearch> at commit
  `1ed183eb37e910136bbd68714904c062cc5b63b4`, the studio pattern in
  `server/internal/filter/wordlists.go`, Copyright (c) 2026 Max Lv, licensed under the MIT
  License.
- `dan211`: <https://github.com/waifu-project/dan211> at commit
  `feb03266086b61bd62bd6c42b27fe38c2d88d1fe`, the popularity-ranked `assets/buitln/av_studios.json`.
  The repository declares no license; its prefixes were kept only where the MDCx DMM series table
  or subtitle table confirms them, and its terms must be confirmed like the other unlicensed word
  lists above before a public release redistributes them.

The MIT License requires its copyright and permission notice to accompany substantial portions:
Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions: The above copyright notice and this
permission notice shall be included in all copies or substantial portions of the Software. THE
SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED.

Code prefixes were kept when a maker table or filter lists them, when at least two
Chinese-subtitled releases use them, or when a single subtitled release or the dan211 popularity
list names a series that the MDCx DMM series table confirms; the rest of the roughly 11,000 DMM
series were left out. Short forms of
number-prefixed amateur series (`MIUM` for `300MIUM`) were added for well-attested series.
Single-letter prefixes, two-letter prefixes with few releases, and series numbered by date or with
five or more digits (such as `FC2`, `1PON`, and `HODV`) were dropped. Two-letter prefixes, months
and weekdays, currencies and technical abbreviations that commonly precede numbers, and ordinary
English words were marked `ambiguous`, except `JUL`, a major label kept `strict` although it is
also a month abbreviation.

Local changes: terms were filtered to explicit sexual activity and body parts, deduplicated per
language after NFKC normalization and lowercasing, and each term was labelled with a category, a
match mode (`word`, `substring`, `prefix`, or `code`), and a confidence. `strict` terms are
always enforced; `ambiguous` terms also occur in ordinary titles and are enforced only when an
administrator turns them on.

The source lists are community-maintained and are not authoritative linguistic or legal
classifications. Karma therefore treats this file as versioned policy data and keeps title
matching independently configurable.

The other candidate sources listed in
[`docs/browser-policy-and-title-keyword-research.md`](../../docs/browser-policy-and-title-keyword-research.md)
were used for engineering and licensing comparison only and were not copied into this data file.
