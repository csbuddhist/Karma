import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import export_viddexa


class ExportHelpersTest(unittest.TestCase):
    def test_source_must_match_pinned_repository_and_revision(self):
        export_viddexa.validate_source(
            export_viddexa.REPOSITORY,
            export_viddexa.REVISION,
        )

        with self.assertRaises(ValueError):
            export_viddexa.validate_source(export_viddexa.REPOSITORY, "main")
        with self.assertRaises(ValueError):
            export_viddexa.validate_source("other/model", export_viddexa.REVISION)

    def test_sha256_is_lowercase_and_deterministic(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "model.onnx"
            path.write_bytes(b"model")

            self.assertEqual(
                export_viddexa.sha256_file(path),
                "9372c470eeadd5ecd9c3c74c2b3cb633f8e2f2fad799250a0f70d652b6b825e4",
            )

    def test_manifest_matches_rust_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            model = Path(directory) / "model.onnx"
            model.write_bytes(b"model")

            manifest = export_viddexa.build_manifest(
                model,
                input_name="pixel_values",
                output_name="logits",
                labels=["normal", "hentai", "porn", "sexy", "drawing"],
            )

        self.assertEqual(manifest["asset"]["kind"], "image_classifier")
        self.assertEqual(manifest["asset"]["license"], "Apache-2.0")
        self.assertEqual(manifest["source_revision"], export_viddexa.REVISION)
        self.assertEqual(manifest["file_bytes"], 5)
        self.assertEqual(manifest["opset"], 18)
        self.assertEqual(manifest["input"]["shape"], [1, 3, 224, 224])
        self.assertEqual(
            [entry["name"] for entry in manifest["labels"]],
            ["normal", "hentai", "porn", "sexy", "drawing"],
        )
        json.dumps(manifest)


if __name__ == "__main__":
    unittest.main()
