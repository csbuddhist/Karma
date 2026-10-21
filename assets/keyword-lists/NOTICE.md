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
- `Karma`: separately authored multi-word phrases kept from the earlier curated list.

Local changes: terms were filtered to explicit sexual activity and body parts, deduplicated per
language after NFKC normalization and lowercasing, and each term was labelled with a category, a
match mode (`word`, `substring`, or `prefix`), and a confidence. `strict` terms are always
enforced; `ambiguous` terms also occur in ordinary titles and are enforced only when an
administrator turns them on.

The source lists are community-maintained and are not authoritative linguistic or legal
classifications. Karma therefore treats this file as versioned policy data and keeps title
matching independently configurable.

The other candidate sources listed in
[`docs/browser-policy-and-title-keyword-research.md`](../../docs/browser-policy-and-title-keyword-research.md)
were used for engineering and licensing comparison only and were not copied into this data file.
