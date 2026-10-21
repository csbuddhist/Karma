#!/usr/bin/env python3
"""Builds the bundled window-title keyword list from its CSV source.

The CSV (`assets/keyword-lists/window-title-explicit.csv`) is the editable source of truth and
records where each term came from. The generated JSON, which omits that provenance, is embedded by the Rust policy engine and imported by the console, so both
always enforce and display the same list. Run without arguments to regenerate the JSON, or with
`--check` to fail when the committed JSON is stale.
"""

from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path
import re
import sys
import unicodedata

REPOSITORY = Path(__file__).resolve().parents[2]
CSV_PATH = REPOSITORY / "assets/keyword-lists/window-title-explicit.csv"
JSON_PATH = REPOSITORY / "assets/keyword-lists/window-title-explicit.json"

FORMAT_VERSION = 2
FIELDS = ["lang", "term", "category", "confidence", "match", "sources"]
CATEGORIES = {"organ", "sexual"}
CONFIDENCES = {"strict", "ambiguous"}
MATCHES = {"word", "substring", "prefix"}
LANGUAGE = re.compile(r"^[a-z]{2,3}$")
SOURCE = re.compile(r"^[A-Za-z0-9-]{1,32}$")
MAX_TERM_CHARS = 64


class KeywordListError(ValueError):
    pass


def normalized(term: str) -> str:
    return unicodedata.normalize("NFKC", term).lower().strip()


def read_rows(text: str) -> list[dict[str, str]]:
    reader = csv.DictReader(text.splitlines())
    if reader.fieldnames != FIELDS:
        raise KeywordListError(f"CSV header must be {','.join(FIELDS)}")
    rows = []
    seen: set[tuple[str, str]] = set()
    for line, row in enumerate(reader, start=2):
        if None in row or any(value is None for value in row.values()):
            raise KeywordListError(f"line {line}: wrong number of fields")
        term = row["term"]
        if not term or term != term.strip() or len(term) > MAX_TERM_CHARS:
            raise KeywordListError(f"line {line}: term must be 1-{MAX_TERM_CHARS} trimmed characters")
        if not LANGUAGE.match(row["lang"]):
            raise KeywordListError(f"line {line}: invalid language {row['lang']!r}")
        if row["category"] not in CATEGORIES:
            raise KeywordListError(f"line {line}: invalid category {row['category']!r}")
        if row["confidence"] not in CONFIDENCES:
            raise KeywordListError(f"line {line}: invalid confidence {row['confidence']!r}")
        if row["match"] not in MATCHES:
            raise KeywordListError(f"line {line}: invalid match {row['match']!r}")
        sources = row["sources"].split(";")
        if not all(SOURCE.match(source) for source in sources):
            raise KeywordListError(f"line {line}: invalid sources {row['sources']!r}")
        key = (row["lang"], normalized(term))
        if key in seen:
            raise KeywordListError(f"line {line}: duplicate term {term!r} for {row['lang']}")
        seen.add(key)
        rows.append(row)
    if not rows:
        raise KeywordListError("CSV contains no keywords")
    return rows


def render(rows: list[dict[str, str]]) -> str:
    """One keyword object per line keeps reviews of list changes readable."""
    languages: dict[str, list[dict[str, object]]] = {}
    for row in rows:
        languages.setdefault(row["lang"], []).append(
            {
                "term": row["term"],
                "category": row["category"],
                "confidence": row["confidence"],
                "match": row["match"],
            }
        )
    groups = []
    for language in sorted(languages):
        keywords = ",\n".join(
            "        " + json.dumps(keyword, ensure_ascii=False) for keyword in languages[language]
        )
        groups.append(
            "    {\n"
            f'      "language": {json.dumps(language)},\n'
            '      "keywords": [\n'
            f"{keywords}\n"
            "      ]\n"
            "    }"
        )
    return (
        "{\n"
        f'  "format_version": {FORMAT_VERSION},\n'
        '  "languages": [\n'
        + ",\n".join(groups)
        + "\n  ]\n}\n"
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if the JSON is out of date")
    arguments = parser.parse_args(argv)
    try:
        output = render(read_rows(CSV_PATH.read_text(encoding="utf-8")))
    except KeywordListError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    if arguments.check:
        if JSON_PATH.read_text(encoding="utf-8") != output:
            print(f"error: {JSON_PATH.relative_to(REPOSITORY)} is stale; rerun this script", file=sys.stderr)
            return 1
        return 0
    JSON_PATH.write_text(output, encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
