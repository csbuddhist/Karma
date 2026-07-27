#!/usr/bin/env python3
"""Verify a generated PP-OCRv5 bundle without exposing reference values."""

from __future__ import annotations

from array import array
import argparse
import json
import math
import os
from pathlib import Path
import struct
import sys
from typing import Any

import export as exporter


MAXIMUM_MANIFEST_BYTES = 1024 * 1024
MAXIMUM_REFERENCE_BYTES = 256 * 1024 * 1024
TOP_LEVEL_KEYS = {
    "format_version",
    "asset",
    "profile",
    "source_repository",
    "source_revision",
    "detector",
    "recognizer",
    "dictionary",
    "detector_contract",
    "recognizer_contract",
    "thresholds",
    "resource_limits",
    "reference_artifacts",
    "export_toolchain",
    "opset",
    "minimum_runtime_version",
}


def _exact_keys(value: object, expected: set[str], context: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        raise exporter.ExportError(f"{context} keys do not match the runtime schema")
    return value


def _read_manifest(path: Path) -> dict[str, Any]:
    if path.name != "manifest.json":
        raise exporter.ExportError("bundle manifest must be named manifest.json")
    payload = path.read_bytes()
    if not payload or len(payload) > MAXIMUM_MANIFEST_BYTES:
        raise exporter.ExportError("bundle manifest length is invalid")
    value = json.loads(payload)
    return _exact_keys(value, TOP_LEVEL_KEYS, "manifest")


def _verify_artifact(
    directory: Path,
    relative_path: str,
    *,
    expected_bytes: object,
    expected_sha256: object,
) -> None:
    if (
        not isinstance(expected_bytes, int)
        or isinstance(expected_bytes, bool)
        or expected_bytes <= 0
        or not exporter._is_lower_sha256(expected_sha256)
    ):
        raise exporter.ExportError(f"{relative_path} metadata is invalid")
    path = directory / relative_path
    if not path.is_file() or path.is_symlink():
        raise exporter.ExportError(f"{relative_path} is missing or not a regular file")
    exporter.verify_file(path, expected_bytes, expected_sha256)


def _verify_model(
    directory: Path,
    value: object,
    *,
    kind: str,
    file_name: str,
    pin: exporter.ModelPin,
) -> None:
    model = _exact_keys(
        value,
        {"asset", "model_name", "upstream", "file_name", "file_bytes"},
        pin.role,
    )
    asset = _exact_keys(model["asset"], {"kind", "version", "license", "sha256"}, "asset")
    upstream = _exact_keys(
        model["upstream"], {"download_url", "file_bytes", "sha256"}, "upstream"
    )
    if (
        asset["kind"] != kind
        or asset["license"] != "Apache-2.0"
        or not isinstance(asset["version"], str)
        or not asset["version"]
        or model["model_name"] != pin.name
        or model["file_name"] != file_name
        or upstream
        != {
            "download_url": pin.url,
            "file_bytes": pin.file_bytes,
            "sha256": pin.sha256,
        }
    ):
        raise exporter.ExportError(f"{pin.role} manifest metadata is invalid")
    _verify_artifact(
        directory,
        file_name,
        expected_bytes=model["file_bytes"],
        expected_sha256=asset["sha256"],
    )


def _parse_reference_input(path: Path) -> tuple[list[int], array]:
    payload = path.read_bytes()
    if len(payload) < 12 or len(payload) > MAXIMUM_REFERENCE_BYTES:
        raise exporter.ExportError("reference input length is invalid")
    if payload[:4] != b"KOR1":
        raise exporter.ExportError("reference input magic is invalid")
    rank = struct.unpack("<I", payload[4:8])[0]
    if rank == 0 or rank > 8:
        raise exporter.ExportError("reference input rank is invalid")
    header_bytes = 8 + rank * 4
    if len(payload) < header_bytes:
        raise exporter.ExportError("reference input header is truncated")
    shape = list(struct.unpack(f"<{rank}I", payload[8:header_bytes]))
    if any(dimension == 0 for dimension in shape):
        raise exporter.ExportError("reference input shape is invalid")
    expected_bytes = math.prod(shape) * 4
    if len(payload) != header_bytes + expected_bytes:
        raise exporter.ExportError("reference input payload length is invalid")
    values = array("f")
    values.frombytes(payload[header_bytes:])
    if sys.byteorder != "little":
        values.byteswap()
    if any(not math.isfinite(value) for value in values):
        raise exporter.ExportError("reference input contains non-finite values")
    return shape, values


def _parse_reference_output(path: Path) -> tuple[list[int], list[float]]:
    payload = path.read_bytes()
    if not payload or len(payload) > MAXIMUM_REFERENCE_BYTES:
        raise exporter.ExportError("reference output length is invalid")
    value = _exact_keys(json.loads(payload), {"shape", "values"}, "reference output")
    shape = value["shape"]
    values = value["values"]
    if (
        not isinstance(shape, list)
        or not shape
        or len(shape) > 8
        or any(
            not isinstance(dimension, int)
            or isinstance(dimension, bool)
            or dimension <= 0
            for dimension in shape
        )
        or not isinstance(values, list)
        or len(values) != math.prod(shape)
        or any(
            not isinstance(item, (int, float))
            or isinstance(item, bool)
            or not math.isfinite(item)
            for item in values
        )
    ):
        raise exporter.ExportError("reference output tensor is invalid")
    return shape, [float(item) for item in values]


def _verify_reference_metadata(
    directory: Path, references: dict[str, Any]
) -> dict[str, tuple[list[int], Any]]:
    expected_paths = {
        "detector_input": "reference/detector-input.bin",
        "detector_output": "reference/detector-output.json",
        "recognizer_input": "reference/recognizer-input.bin",
        "recognizer_output": "reference/recognizer-output.json",
    }
    _exact_keys(references, set(expected_paths), "reference artifacts")
    parsed: dict[str, tuple[list[int], Any]] = {}
    for name, expected_path in expected_paths.items():
        artifact = _exact_keys(
            references[name], {"path", "file_bytes", "sha256"}, name
        )
        if artifact["path"] != expected_path:
            raise exporter.ExportError(f"{name} path is invalid")
        _verify_artifact(
            directory,
            expected_path,
            expected_bytes=artifact["file_bytes"],
            expected_sha256=artifact["sha256"],
        )
        if name.endswith("_input"):
            parsed[name] = _parse_reference_input(directory / expected_path)
        else:
            parsed[name] = _parse_reference_output(directory / expected_path)
    return parsed


def _verify_onnx_reference(
    model_path: Path,
    contract: dict[str, Any],
    reference_input: tuple[list[int], array],
    reference_output: tuple[list[int], list[float]],
) -> None:
    try:
        import numpy as np
        import onnxruntime
    except ImportError as error:
        raise exporter.ExportError("numpy and onnxruntime are required to verify references") from error

    input_shape, input_values = reference_input
    expected_shape, expected_values = reference_output
    tensor = np.asarray(input_values, dtype=np.float32).reshape(input_shape)
    options = onnxruntime.SessionOptions()
    options.intra_op_num_threads = 1
    options.graph_optimization_level = (
        onnxruntime.GraphOptimizationLevel.ORT_ENABLE_ALL
    )
    session = onnxruntime.InferenceSession(
        os.fspath(model_path),
        sess_options=options,
        providers=["CPUExecutionProvider"],
    )
    actual = session.run(
        [contract["output_name"]], {contract["input_name"]: tensor}
    )[0]
    expected = np.asarray(expected_values, dtype=np.float32).reshape(expected_shape)
    if actual.shape != expected.shape or not np.isfinite(actual).all():
        raise exporter.ExportError("ONNX reference output shape or values are invalid")
    if np.any(np.abs(actual - expected) > exporter.REFERENCE_TOLERANCE):
        raise exporter.ExportError("ONNX reference output does not match")


def _verify_exact_files(directory: Path) -> None:
    expected = {"manifest.json", *exporter.BUNDLE_DIGEST_PATHS}
    actual: set[str] = set()
    for path in directory.rglob("*"):
        if path.is_symlink():
            raise exporter.ExportError("bundle contains a symbolic link")
        if path.is_file():
            actual.add(path.relative_to(directory).as_posix())
    if actual != expected:
        raise exporter.ExportError("bundle contains missing or unexpected files")


def verify_bundle(manifest_path: Path) -> tuple[str, str]:
    manifest_path = manifest_path.expanduser().resolve()
    directory = manifest_path.parent
    manifest = _read_manifest(manifest_path)
    _verify_exact_files(directory)
    configuration = exporter.load_model_config()
    profile = manifest["profile"]
    if profile not in configuration.profiles:
        raise exporter.ExportError("bundle profile is invalid")
    if (
        manifest["format_version"] != 1
        or manifest["source_repository"] != configuration.repository
        or manifest["source_revision"] != configuration.revision
        or manifest["opset"] != exporter.OPSET
        or manifest["minimum_runtime_version"] != "1.22"
    ):
        raise exporter.ExportError("bundle root contract is invalid")

    profile_models = configuration.profiles[profile]
    detector_pin = configuration.models[profile_models["detector"]]
    recognizer_pin = configuration.models[profile_models["recognizer"]]
    _verify_model(
        directory,
        manifest["detector"],
        kind="ocr_detector",
        file_name="detector.onnx",
        pin=detector_pin,
    )
    _verify_model(
        directory,
        manifest["recognizer"],
        kind="ocr_recognizer",
        file_name="recognizer.onnx",
        pin=recognizer_pin,
    )

    dictionary = _exact_keys(
        manifest["dictionary"],
        {"asset", "file_name", "file_bytes", "entries", "blank_index", "languages"},
        "dictionary",
    )
    dictionary_asset = _exact_keys(
        dictionary["asset"], {"kind", "version", "license", "sha256"}, "dictionary asset"
    )
    if (
        dictionary_asset["kind"] != "ocr_dictionary"
        or dictionary_asset["license"] != "Apache-2.0"
        or dictionary["file_name"] != "dictionary.txt"
        or dictionary["blank_index"] != 0
        or dictionary["languages"]
        != ["english", "chinese_simplified", "chinese_traditional"]
        or not isinstance(dictionary["entries"], list)
        or not dictionary["entries"]
    ):
        raise exporter.ExportError("dictionary manifest is invalid")
    _verify_artifact(
        directory,
        "dictionary.txt",
        expected_bytes=dictionary["file_bytes"],
        expected_sha256=dictionary_asset["sha256"],
    )
    if (directory / "dictionary.txt").read_text(encoding="utf-8").splitlines() != dictionary[
        "entries"
    ]:
        raise exporter.ExportError("dictionary bytes do not match manifest entries")

    asset = _exact_keys(
        manifest["asset"], {"kind", "version", "license", "sha256"}, "bundle asset"
    )
    if (
        asset["kind"] != "ocr_bundle"
        or asset["license"] != "Apache-2.0"
        or not isinstance(asset["version"], str)
        or not asset["version"]
        or asset["sha256"] != exporter.bundle_digest(directory)
    ):
        raise exporter.ExportError("bundle aggregate digest is invalid")

    expected_detector_contract = {
        "layout": "nchw",
        "color_order": "rgb",
        "element_type": "f32",
        "channels": 3,
        "minimum_height": 32,
        "maximum_height": 640,
        "minimum_width": 32,
        "maximum_width": 640,
        "dimension_multiple": 32,
        "scale": 1.0 / 255.0,
        "mean": [0.485, 0.456, 0.406],
        "std": [0.229, 0.224, 0.225],
    }
    expected_recognizer_contract = {
        "layout": "nchw",
        "color_order": "rgb",
        "element_type": "f32",
        "channels": 3,
        "minimum_height": 48,
        "maximum_height": 48,
        "minimum_width": 1,
        "maximum_width": 320,
        "dimension_multiple": 1,
        "scale": 1.0 / 255.0,
        "mean": [0.5, 0.5, 0.5],
        "std": [0.5, 0.5, 0.5],
    }
    detector_contract = _exact_keys(
        manifest["detector_contract"],
        {"input_name", "output_name", *expected_detector_contract},
        "detector contract",
    )
    recognizer_contract = _exact_keys(
        manifest["recognizer_contract"],
        {"input_name", "output_name", *expected_recognizer_contract},
        "recognizer contract",
    )
    if (
        {key: detector_contract[key] for key in expected_detector_contract}
        != expected_detector_contract
        or {key: recognizer_contract[key] for key in expected_recognizer_contract}
        != expected_recognizer_contract
    ):
        raise exporter.ExportError("tensor preprocessing contract is invalid")
    detector_graph = exporter.check_onnx_contract(
        directory / "detector.onnx", "detector"
    )
    recognizer_graph = exporter.check_onnx_contract(
        directory / "recognizer.onnx", "recognizer"
    )
    if detector_graph[:2] != (
        detector_contract["input_name"],
        detector_contract["output_name"],
    ) or recognizer_graph[:2] != (
        recognizer_contract["input_name"],
        recognizer_contract["output_name"],
    ):
        raise exporter.ExportError("manifest tensor names do not match ONNX")
    if recognizer_graph[2] != len(dictionary["entries"]) + 1:
        raise exporter.ExportError("recognizer classes do not match dictionary")

    if manifest["thresholds"] != {
        "probability": 0.3,
        "text_box": 0.6,
        "expansion": 1.5,
        "recognition_confidence": 0.5,
    } or manifest["resource_limits"] != {
        "maximum_text_boxes": 64,
        "minimum_box_side_pixels": 6,
        "minimum_box_area_pixels": 48,
        "recognizer_height": 48,
        "maximum_recognizer_width": 320,
        "maximum_batch_size": 8,
        "maximum_line_characters": 128,
        "maximum_total_characters": 4096,
    }:
        raise exporter.ExportError("threshold or resource contract is invalid")

    toolchain = _exact_keys(
        manifest["export_toolchain"],
        {
            "paddlepaddle_version",
            "paddle2onnx_version",
            "onnx_version",
            "onnx_runtime_version",
        },
        "export toolchain",
    )
    installed_versions = {
        "paddlepaddle_version": exporter._package_version("paddlepaddle"),
        "paddle2onnx_version": exporter._package_version("paddle2onnx"),
        "onnx_version": exporter._package_version("onnx"),
        "onnx_runtime_version": exporter._package_version("onnxruntime"),
    }
    if toolchain != installed_versions:
        raise exporter.ExportError("manifest toolchain does not match verifier environment")

    references = _verify_reference_metadata(
        directory, manifest["reference_artifacts"]
    )
    _verify_onnx_reference(
        directory / "detector.onnx",
        detector_contract,
        references["detector_input"],
        references["detector_output"],
    )
    _verify_onnx_reference(
        directory / "recognizer.onnx",
        recognizer_contract,
        references["recognizer_input"],
        references["recognizer_output"],
    )
    return profile, asset["version"]


def build_argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Verify an exported PP-OCRv5 bundle and its numerical references."
    )
    parser.add_argument("manifest", type=Path)
    return parser


def main() -> int:
    arguments = build_argument_parser().parse_args()
    try:
        profile, version = verify_bundle(arguments.manifest)
    except (exporter.ExportError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"status=failed error={type(error).__name__}", file=sys.stderr)
        return 1
    print(f"status=verified profile={profile} version={version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
