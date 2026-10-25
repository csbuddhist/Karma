from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import unittest


TOOL_PATH = Path(__file__).resolve().parents[1] / "build_title_keywords.py"


def load_builder():
    spec = importlib.util.spec_from_file_location("karma_title_keywords", TOOL_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("unable to load keyword builder")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


builder = load_builder()
HEADER = "lang,term,category,confidence,match,sources\n"


class BuildTitleKeywordsTest(unittest.TestCase):
    def test_render_groups_keywords_by_language(self):
        rows = builder.read_rows(
            HEADER
            + "zh,色情,sexual,strict,substring,LDNOOBW;censor-text\n"
            + "en,porn,sexual,strict,word,LDNOOBW\n"
            + "en,lesb,sexual,ambiguous,prefix,censor-text\n"
        )
        document = json.loads(builder.render(rows))
        self.assertEqual(document["format_version"], 2)
        self.assertEqual([group["language"] for group in document["languages"]], ["en", "zh"])
        self.assertEqual(
            document["languages"][0]["keywords"][1],
            {"term": "lesb", "category": "sexual", "confidence": "ambiguous", "match": "prefix"},
        )

    def test_duplicates_after_normalization_are_rejected(self):
        with self.assertRaisesRegex(builder.KeywordListError, "duplicate"):
            builder.read_rows(HEADER + "en,porn,sexual,strict,word,A\n" + "en,ＰＯＲＮ,sexual,strict,word,B\n")

    def test_unknown_enumerations_are_rejected(self):
        for row in [
            "en,porn,violence,strict,word,A",
            "en,porn,sexual,maybe,word,A",
            "en,porn,sexual,strict,regex,A",
            "english,porn,sexual,strict,word,A",
            "en, porn,sexual,strict,word,A",
            "en,porn,sexual,strict,word,bad source",
        ]:
            with self.subTest(row=row), self.assertRaises(builder.KeywordListError):
                builder.read_rows(HEADER + row + "\n")

    def test_code_prefixes_require_code_matching_and_ascii_terms(self):
        rows = builder.read_rows(HEADER + "ja,SSIS,code,strict,code,MDCx\n" + "ja,300MIUM,code,strict,code,MDCx\n")
        self.assertEqual([row["term"] for row in rows], ["SSIS", "300MIUM"])
        for row in [
            "ja,SSIS,code,strict,word,A",
            "ja,SSIS,sexual,strict,code,A",
            "ja,1234,code,strict,code,A",
            "ja,S,code,strict,code,A",
            "ja,番号,code,strict,code,A",
            "ja,SS IS,code,strict,code,A",
            "ja," + "S" * 17 + ",code,strict,code,A",
        ]:
            with self.subTest(row=row), self.assertRaises(builder.KeywordListError):
                builder.read_rows(HEADER + row + "\n")

    def test_committed_json_matches_csv_source(self):
        self.assertEqual(builder.main(["--check"]), 0)


if __name__ == "__main__":
    unittest.main()
