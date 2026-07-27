#!/usr/bin/env python3
"""Reproducibly export pinned official PP-OCRv5 inference bundles to ONNX."""

from __future__ import annotations

import argparse
from array import array
from dataclasses import dataclass
import hashlib
import importlib.metadata
import json
import math
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from typing import Any, Iterable, Sequence
from urllib.parse import urlsplit
from urllib.request import Request, urlopen


TOOL_DIRECTORY = Path(__file__).resolve().parent
REPOSITORY_ROOT = TOOL_DIRECTORY.parents[1]
MODELS_PATH = TOOL_DIRECTORY / "models.toml"
ATTRIBUTION_DIRECTORY = REPOSITORY_ROOT / "assets" / "ocr" / "pp-ocrv5-mobile"
OFFICIAL_REPOSITORY = "https://github.com/PaddlePaddle/PaddleOCR"
OFFICIAL_MODEL_HOST = "paddle-model-ecology.bj.bcebos.com"
ARCHIVE_LIMIT_BYTES = 256 * 1024 * 1024
EXTRACTED_LIMIT_BYTES = 512 * 1024 * 1024
MAXIMUM_ARCHIVE_MEMBERS = 64
OPSET = 18
REFERENCE_TOLERANCE = 1.0e-4
PADDLE_COMPARISON_ATOL = 1.0e-4
PADDLE_COMPARISON_RTOL = 3.0e-4
EXPECTED_TOOLCHAIN = {
    "python_implementation": "cpython",
    "python_version": "3.11",
    "paddlepaddle_version": "3.0.0",
    "paddle2onnx_version": "2.1.0",
    "onnx_version": "1.17.0",
    "onnx_runtime_version": "1.22.0",
}
TOOLCHAIN_DISTRIBUTIONS = {
    "paddlepaddle_version": "paddlepaddle",
    "paddle2onnx_version": "paddle2onnx",
    "onnx_version": "onnx",
    "onnx_runtime_version": "onnxruntime",
}
BUNDLE_DIGEST_PATHS = (
    "LICENSE",
    "NOTICE.md",
    "detector.onnx",
    "recognizer.onnx",
    "dictionary.txt",
    "reference/detector-input.bin",
    "reference/detector-output.json",
    "reference/recognizer-input.bin",
    "reference/recognizer-output.json",
)


class ExportError(RuntimeError):
    """A stable export failure that contains no model input or output values."""


@dataclass(frozen=True)
class ModelPin:
    name: str
    role: str
    url: str
    file_bytes: int
    sha256: str


@dataclass(frozen=True)
class ExportConfiguration:
    repository: str
    revision: str
    release: str
    documentation_url: str
    toolchain: dict[str, str]
    profiles: dict[str, dict[str, str]]
    models: dict[str, ModelPin]


def _is_lower_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def _validate_official_url(value: object, model_name: str) -> str:
    if not isinstance(value, str):
        raise ExportError("model URL must be a string")
    parsed = urlsplit(value)
    expected_path = (
        "/paddlex/official_inference_model/paddle3.0.0/"
        f"{model_name}_infer.tar"
    )
    if (
        parsed.scheme != "https"
        or parsed.hostname != OFFICIAL_MODEL_HOST
        or parsed.port is not None
        or parsed.username is not None
        or parsed.password is not None
        or parsed.path != expected_path
        or parsed.query
        or parsed.fragment
    ):
        raise ExportError(f"{model_name} does not use its exact official HTTPS URL")
    return value


def load_model_config(path: Path = MODELS_PATH) -> ExportConfiguration:
    raw = tomllib.loads(path.read_text(encoding="utf-8"))
    if set(raw) != {"provenance", "toolchain", "profiles", "models"}:
        raise ExportError("models.toml has unexpected top-level keys")
    provenance = raw["provenance"]
    if set(provenance) != {
        "repository",
        "revision",
        "release",
        "documentation_url",
    }:
        raise ExportError("models.toml provenance keys are invalid")
    repository = provenance["repository"]
    revision = provenance["revision"]
    release = provenance["release"]
    documentation_url = provenance["documentation_url"]
    if repository != OFFICIAL_REPOSITORY:
        raise ExportError("PaddleOCR repository is not official")
    if (
        not isinstance(revision, str)
        or len(revision) != 40
        or any(character not in "0123456789abcdef" for character in revision)
    ):
        raise ExportError("PaddleOCR revision is not an immutable commit")
    expected_documentation = (
        f"{OFFICIAL_REPOSITORY}/blob/{revision}/"
        "docs/version3.x/pipeline_usage/OCR.en.md"
    )
    if documentation_url != expected_documentation:
        raise ExportError("PaddleOCR documentation is not pinned to the source commit")
    if not isinstance(release, str) or not release.startswith("v"):
        raise ExportError("PaddleOCR release is invalid")
    toolchain = raw["toolchain"]
    if toolchain != EXPECTED_TOOLCHAIN:
        raise ExportError("models.toml toolchain pins are invalid")
    locked_requirements = {}
    for line in (TOOL_DIRECTORY / "requirements.lock").read_text(
        encoding="utf-8"
    ).splitlines():
        if line and not line.startswith(("#", " ", "\t", "--")) and "==" in line:
            name, version = line.split("==", 1)
            locked_requirements[name] = version.removesuffix(" \\")
    for key, distribution in TOOLCHAIN_DISTRIBUTIONS.items():
        if locked_requirements.get(distribution) != toolchain[key]:
            raise ExportError("models.toml toolchain does not match requirements.lock")

    expected_models = {
        "PP-OCRv5_mobile_det": "detector",
        "PP-OCRv5_mobile_rec": "recognizer",
        "PP-OCRv5_server_det": "detector",
        "PP-OCRv5_server_rec": "recognizer",
    }
    model_values = raw["models"]
    if set(model_values) != set(expected_models):
        raise ExportError("models.toml must pin exactly four PP-OCRv5 models")
    models: dict[str, ModelPin] = {}
    for model_name, expected_role in expected_models.items():
        value = model_values[model_name]
        if set(value) != {"role", "url", "file_bytes", "sha256"}:
            raise ExportError(f"{model_name} has unexpected pin fields")
        file_bytes = value["file_bytes"]
        sha256 = value["sha256"]
        if value["role"] != expected_role:
            raise ExportError(f"{model_name} has the wrong role")
        if (
            not isinstance(file_bytes, int)
            or isinstance(file_bytes, bool)
            or file_bytes <= 0
            or file_bytes > ARCHIVE_LIMIT_BYTES
        ):
            raise ExportError(f"{model_name} has an invalid byte length")
        if not _is_lower_sha256(sha256):
            raise ExportError(f"{model_name} has an invalid SHA-256")
        models[model_name] = ModelPin(
            name=model_name,
            role=expected_role,
            url=_validate_official_url(value["url"], model_name),
            file_bytes=file_bytes,
            sha256=sha256,
        )

    expected_profiles = {
        "lightweight": {
            "detector": "PP-OCRv5_mobile_det",
            "recognizer": "PP-OCRv5_mobile_rec",
        },
        "accurate": {
            "detector": "PP-OCRv5_server_det",
            "recognizer": "PP-OCRv5_server_rec",
        },
    }
    if raw["profiles"] != expected_profiles:
        raise ExportError("models.toml profile mapping is invalid")
    return ExportConfiguration(
        repository=repository,
        revision=revision,
        release=release,
        documentation_url=documentation_url,
        toolchain=dict(toolchain),
        profiles=raw["profiles"],
        models=models,
    )


def require_production_python() -> None:
    if (
        sys.implementation.name != EXPECTED_TOOLCHAIN["python_implementation"]
        or sys.version_info[:2] != (3, 11)
    ):
        raise ExportError("production OCR tooling requires exactly CPython 3.11")


def validate_installed_toolchain(
    configuration: ExportConfiguration,
) -> dict[str, str]:
    require_production_python()
    installed = {
        key: _package_version(distribution)
        for key, distribution in TOOLCHAIN_DISTRIBUTIONS.items()
    }
    expected = {
        key: configuration.toolchain[key]
        for key in TOOLCHAIN_DISTRIBUTIONS
    }
    if installed != expected:
        raise ExportError("installed OCR export toolchain does not match reviewed pins")
    return installed


def artifact_metadata(path: Path) -> dict[str, int | str]:
    digest = hashlib.sha256()
    length = 0
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            length += len(chunk)
            digest.update(chunk)
    return {"file_bytes": length, "sha256": digest.hexdigest()}


def verify_file(path: Path, expected_bytes: int, expected_sha256: str) -> None:
    metadata = artifact_metadata(path)
    if metadata != {
        "file_bytes": expected_bytes,
        "sha256": expected_sha256,
    }:
        raise ExportError(f"integrity verification failed for {path.name}")


def download_model(pin: ModelPin, cache_directory: Path) -> Path:
    cache_directory.mkdir(parents=True, exist_ok=True)
    archive = cache_directory / f"{pin.name}_infer.tar"
    if archive.exists():
        verify_file(archive, pin.file_bytes, pin.sha256)
        return archive

    partial = archive.with_suffix(".tar.part")
    if partial.exists():
        partial.unlink()
    request = Request(
        pin.url,
        headers={"User-Agent": "Karma-PP-OCRv5-export/1"},
        method="GET",
    )
    digest = hashlib.sha256()
    length = 0
    try:
        with urlopen(request, timeout=60) as response, partial.open("xb") as target:
            if _validate_official_url(response.geturl(), pin.name) != pin.url:
                raise ExportError(f"{pin.name} download redirected unexpectedly")
            while chunk := response.read(1024 * 1024):
                length += len(chunk)
                if length > pin.file_bytes or length > ARCHIVE_LIMIT_BYTES:
                    raise ExportError(f"{pin.name} download exceeded its pinned length")
                digest.update(chunk)
                target.write(chunk)
        if length != pin.file_bytes or digest.hexdigest() != pin.sha256:
            raise ExportError(f"{pin.name} download did not match its reviewed pin")
        partial.replace(archive)
    except BaseException:
        partial.unlink(missing_ok=True)
        raise
    return archive


def _validated_member_path(member: tarfile.TarInfo) -> PurePosixPath:
    name = member.name.rstrip("/") if member.isdir() else member.name
    if (
        not name
        or "\\" in name
        or "\x00" in name
        or name.startswith("/")
        or any(part in {"", ".", ".."} for part in PurePosixPath(name).parts)
        or any(":" in part for part in PurePosixPath(name).parts)
    ):
        raise ExportError("archive contains an unsafe path")
    path = PurePosixPath(name)
    if path.is_absolute():
        raise ExportError("archive contains an absolute path")
    return path


def safe_extract_tar(archive_path: Path, destination: Path) -> None:
    """Extract a small tar after validating every member before any filesystem write."""
    if destination.exists():
        raise ExportError("extraction destination already exists")
    with tarfile.open(archive_path, mode="r:") as archive:
        members = archive.getmembers()
        if not members or len(members) > MAXIMUM_ARCHIVE_MEMBERS:
            raise ExportError("archive member count is invalid")
        validated: list[tuple[tarfile.TarInfo, PurePosixPath]] = []
        seen: set[PurePosixPath] = set()
        extracted_bytes = 0
        for member in members:
            path = _validated_member_path(member)
            if path in seen:
                raise ExportError("archive contains a duplicate path")
            seen.add(path)
            if _is_sparse_member(member):
                raise ExportError("archive contains a sparse file")
            if not (member.isdir() or member.isreg()):
                raise ExportError("archive contains a link or special file")
            if member.size < 0:
                raise ExportError("archive member size is invalid")
            extracted_bytes += member.size
            if extracted_bytes > EXTRACTED_LIMIT_BYTES:
                raise ExportError("archive expands beyond the extraction limit")
            validated.append((member, path))

        regular_paths = {
            path for member, path in validated if member.isreg()
        }
        if any(
            parent in regular_paths
            for _, path in validated
            for parent in path.parents
            if parent != PurePosixPath(".")
        ):
            raise ExportError("archive nests a member beneath a regular file")

        destination.mkdir(parents=True)
        for member, path in sorted(
            validated, key=lambda item: (len(item[1].parts), item[0].isreg())
        ):
            target = destination.joinpath(*path.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=False)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            source = archive.extractfile(member)
            if source is None:
                raise ExportError("regular archive member has no payload")
            remaining = member.size
            with source, target.open("xb") as output:
                while remaining:
                    chunk = source.read(min(1024 * 1024, remaining))
                    if not chunk:
                        raise ExportError("archive member ended before its declared length")
                    output.write(chunk)
                    remaining -= len(chunk)
                if source.read(1):
                    raise ExportError("archive member exceeded its declared length")


def _is_sparse_member(member: tarfile.TarInfo) -> bool:
    return (
        member.type == tarfile.GNUTYPE_SPARSE
        or member.issparse()
        or getattr(member, "sparse", None) is not None
        or any(key.startswith("GNU.sparse") for key in member.pax_headers)
    )


def run_checked(
    command: Sequence[str | os.PathLike[str]],
    *,
    cwd: Path | None = None,
) -> None:
    subprocess.run(
        [os.fspath(part) for part in command],
        check=True,
        cwd=cwd,
        env=os.environ.copy(),
    )


def build_converter_command(
    executable: Path, model_directory: Path, output_path: Path
) -> list[str]:
    return [
        os.fspath(executable),
        "--model_dir",
        os.fspath(model_directory),
        "--model_filename",
        "inference.json",
        "--params_filename",
        "inference.pdiparams",
        "--save_file",
        os.fspath(output_path),
        "--opset_version",
        str(OPSET),
        "--enable_onnx_checker",
        "True",
        "--optimize_tool",
        "None",
    ]


def _validated_shape(shape: Sequence[int]) -> list[int]:
    result = list(shape)
    if (
        not result
        or len(result) > 8
        or any(
            not isinstance(dimension, int)
            or isinstance(dimension, bool)
            or dimension <= 0
            or dimension > 0xFFFFFFFF
            for dimension in result
        )
    ):
        raise ExportError("reference tensor shape is invalid")
    return result


def _finite_values(values: Iterable[float]) -> array:
    result = array("f")
    for value in values:
        converted = float(value)
        if not math.isfinite(converted):
            raise ExportError("reference tensor contains a non-finite value")
        result.append(converted)
    return result


def encode_reference_tensor(shape: Sequence[int], values: Iterable[float]) -> bytes:
    dimensions = _validated_shape(shape)
    expected_values = math.prod(dimensions)
    encoded_values = _finite_values(values)
    if len(encoded_values) != expected_values:
        raise ExportError("reference tensor value count does not match its shape")
    if sys.byteorder != "little":
        encoded_values.byteswap()
    header = b"KOR1" + struct.pack("<I", len(dimensions))
    header += b"".join(struct.pack("<I", dimension) for dimension in dimensions)
    return header + encoded_values.tobytes()


def encode_reference_output(shape: Sequence[int], values: Iterable[float]) -> bytes:
    dimensions = _validated_shape(shape)
    finite_values = _finite_values(values)
    if len(finite_values) != math.prod(dimensions):
        raise ExportError("reference output value count does not match its shape")
    payload = {
        "shape": dimensions,
        "values": list(finite_values),
    }
    return (
        json.dumps(
            payload,
            allow_nan=False,
            ensure_ascii=False,
            separators=(",", ":"),
        )
        + "\n"
    ).encode("utf-8")


def _package_version(distribution: str) -> str:
    try:
        return importlib.metadata.version(distribution)
    except importlib.metadata.PackageNotFoundError as error:
        raise ExportError(f"required package is unavailable: {distribution}") from error


def _paddle2onnx_executable() -> Path:
    if sys.prefix == getattr(sys, "base_prefix", sys.prefix):
        raise ExportError("paddle2onnx requires an active virtual environment")
    try:
        prefix = Path(sys.prefix).resolve(strict=True)
    except OSError as error:
        raise ExportError("active virtual environment is unavailable") from error
    binary_directory = prefix / ("Scripts" if os.name == "nt" else "bin")
    name = "paddle2onnx.exe" if os.name == "nt" else "paddle2onnx"
    candidate = binary_directory / name
    try:
        binary_mode = binary_directory.stat(follow_symlinks=False).st_mode
        mode = candidate.stat(follow_symlinks=False).st_mode
    except OSError as error:
        raise ExportError("paddle2onnx is unavailable inside the active venv") from error
    if (
        not stat.S_ISDIR(binary_mode)
        or not stat.S_ISREG(mode)
        or not os.access(candidate, os.X_OK)
    ):
        raise ExportError("paddle2onnx inside the active venv is not executable")
    return candidate


def _load_yaml(model_directory: Path) -> dict[str, Any]:
    try:
        import yaml
    except ImportError as error:
        raise ExportError("PyYAML is required for official inference metadata") from error
    path = model_directory / "inference.yml"
    value = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ExportError("official inference.yml is invalid")
    return value


def _validate_model_directory(model_directory: Path, pin: ModelPin) -> dict[str, Any]:
    if not model_directory.is_dir():
        raise ExportError(f"{pin.name} archive root is missing")
    expected_files = {"inference.json", "inference.pdiparams", "inference.yml"}
    if {path.name for path in model_directory.iterdir()} != expected_files:
        raise ExportError(f"{pin.name} archive contents are unexpected")
    metadata = _load_yaml(model_directory)
    if metadata.get("Global", {}).get("model_name") != pin.name:
        raise ExportError(f"{pin.name} archive declares a different model")
    return metadata


def _dimension_value(dimension: Any) -> int | None:
    if dimension.HasField("dim_value") and dimension.dim_value > 0:
        return int(dimension.dim_value)
    return None


def _tensor_shape(value_info: Any) -> list[int | None]:
    tensor = value_info.type.tensor_type
    return [_dimension_value(dimension) for dimension in tensor.shape.dim]


def detector_shape_contract(
    input_shape: Sequence[int | None], output_shape: Sequence[int | None]
) -> tuple[list[int | None], list[int | None]]:
    if list(input_shape) != [None, 3, None, None] or list(output_shape) != [
        None,
        1,
        None,
        None,
    ]:
        raise ExportError("raw detector ONNX shapes are not fully dynamic spatially")
    return [1, 3, None, None], [1, 1, None, None]


def constrain_detector_batch(path: Path) -> None:
    """Fix only the detector's single-image batch metadata after dynamic export."""
    try:
        import onnx
    except ImportError as error:
        raise ExportError("onnx is required to constrain detector batch") from error
    model = onnx.load(path)
    if len(model.graph.input) != 1 or len(model.graph.output) != 1:
        raise ExportError("detector ONNX must have one input and one output")
    detector_shape_contract(
        _tensor_shape(model.graph.input[0]), _tensor_shape(model.graph.output[0])
    )
    for value_info in (model.graph.input[0], model.graph.output[0]):
        batch = value_info.type.tensor_type.shape.dim[0]
        batch.ClearField("dim_param")
        batch.dim_value = 1
    onnx.checker.check_model(model)
    path.write_bytes(model.SerializeToString(deterministic=True))


def clamp_detector_probability_output(path: Path) -> None:
    """Clamp detector roundoff to the runtime's strict probability contract."""
    try:
        import onnx
    except ImportError as error:
        raise ExportError("onnx is required to clamp detector probabilities") from error
    model = onnx.load(path)
    if len(model.graph.output) != 1:
        raise ExportError("detector ONNX must have one output")
    output_name = model.graph.output[0].name
    producers = [
        node
        for node in model.graph.node
        if output_name in node.output
    ]
    consumers = [
        node
        for node in model.graph.node
        if output_name in node.input
    ]
    if len(producers) != 1 or consumers:
        raise ExportError("detector output must have one terminal producer")

    unclamped_name = f"{output_name}.unclamped"
    minimum_name = "karma.detector.probability.minimum"
    maximum_name = "karma.detector.probability.maximum"
    reserved_names = {unclamped_name, minimum_name, maximum_name}
    used_names = {
        name
        for node in model.graph.node
        for name in (*node.input, *node.output)
    }
    used_names.update(
        value.name
        for collection_name in ("input", "initializer", "value_info")
        for value in getattr(model.graph, collection_name, ())
    )
    if reserved_names & used_names:
        raise ExportError("detector clamp tensor names already exist")

    producer = producers[0]
    producer.output[:] = [
        unclamped_name if name == output_name else name
        for name in producer.output
    ]
    model.graph.initializer.extend(
        [
            onnx.helper.make_tensor(
                minimum_name, onnx.TensorProto.FLOAT, [], [0.0]
            ),
            onnx.helper.make_tensor(
                maximum_name, onnx.TensorProto.FLOAT, [], [1.0]
            ),
        ]
    )
    model.graph.node.extend(
        [
            onnx.helper.make_node(
                "Clip",
                [unclamped_name, minimum_name, maximum_name],
                [output_name],
                name="KarmaDetectorProbabilityClip",
            )
        ]
    )
    onnx.checker.check_model(model)
    path.write_bytes(model.SerializeToString(deterministic=True))


def _has_detector_probability_clamp(model: Any, onnx: Any) -> bool:
    output_name = model.graph.output[0].name
    producers = [
        node for node in model.graph.node if output_name in node.output
    ]
    if len(producers) != 1:
        return False
    producer = producers[0]
    if producer.op_type != "Clip" or len(producer.input) != 3:
        return False
    initializers = {value.name: value for value in model.graph.initializer}
    try:
        minimum = onnx.numpy_helper.to_array(initializers[producer.input[1]])
        maximum = onnx.numpy_helper.to_array(initializers[producer.input[2]])
    except (KeyError, TypeError, ValueError):
        return False
    return (
        minimum.shape == ()
        and maximum.shape == ()
        and float(minimum) == 0.0
        and float(maximum) == 1.0
    )


def check_onnx_contract(path: Path, role: str) -> tuple[str, str, int | None]:
    try:
        import onnx
    except ImportError as error:
        raise ExportError("onnx is required to validate converted models") from error
    model = onnx.load(path)
    onnx.checker.check_model(model)
    default_opsets = [
        item.version for item in model.opset_import if item.domain in {"", "ai.onnx"}
    ]
    if default_opsets != [OPSET]:
        raise ExportError("converted model does not use exactly ONNX opset 18")
    if len(model.graph.input) != 1 or len(model.graph.output) != 1:
        raise ExportError("converted model must have one input and one output")
    input_value = model.graph.input[0]
    output_value = model.graph.output[0]
    if (
        input_value.type.tensor_type.elem_type != onnx.TensorProto.FLOAT
        or output_value.type.tensor_type.elem_type != onnx.TensorProto.FLOAT
    ):
        raise ExportError("converted model input and output must be TensorProto.FLOAT")
    input_shape = _tensor_shape(input_value)
    output_shape = _tensor_shape(output_value)
    class_count = None
    if role == "detector":
        if input_shape != [1, 3, None, None] or output_shape != [1, 1, None, None]:
            raise ExportError("detector ONNX shapes are not the required dynamic contract")
        if not _has_detector_probability_clamp(model, onnx):
            raise ExportError("detector ONNX does not clamp probabilities to [0, 1]")
    elif role == "recognizer":
        if (
            input_shape != [None, 3, 48, None]
            or len(output_shape) != 3
            or output_shape[:2] != [None, None]
            or not isinstance(output_shape[2], int)
        ):
            raise ExportError("recognizer ONNX shapes are not the required dynamic contract")
        class_count = output_shape[2]
    else:
        raise ExportError("unknown model role")
    return input_value.name, output_value.name, class_count


def _bitmap_glyphs() -> dict[str, tuple[str, ...]]:
    return {
        "安": (
            "000010000",
            "011111110",
            "010000010",
            "000010000",
            "111111111",
            "001010000",
            "000100000",
            "001010000",
            "110001110",
        ),
        "全": (
            "000010000",
            "000101000",
            "001000100",
            "010000010",
            "001111100",
            "000010000",
            "001111100",
            "000010000",
            "011111110",
        ),
        "繁": (
            "010101010",
            "111111111",
            "010101010",
            "111010111",
            "001101100",
            "011111110",
            "000010000",
            "010111010",
            "100010001",
        ),
        "體": (
            "111010111",
            "101111101",
            "111010111",
            "101111101",
            "111000111",
            "010111010",
            "111010111",
            "010111010",
            "111010111",
        ),
        "S": (
            "01110",
            "10001",
            "10000",
            "01110",
            "00001",
            "10001",
            "01110",
        ),
        "A": (
            "01110",
            "10001",
            "10001",
            "11111",
            "10001",
            "10001",
            "10001",
        ),
        "F": (
            "11111",
            "10000",
            "10000",
            "11110",
            "10000",
            "10000",
            "10000",
        ),
        "E": (
            "11111",
            "10000",
            "10000",
            "11110",
            "10000",
            "10000",
            "11111",
        ),
    }


def _draw_bitmap_text(
    image: Any,
    text: str,
    *,
    x: int,
    y: int,
    scale: int,
    color: tuple[int, int, int],
) -> None:
    glyphs = _bitmap_glyphs()
    cursor = x
    for character in text:
        glyph = glyphs[character]
        for row, bits in enumerate(glyph):
            for column, bit in enumerate(bits):
                if bit == "1":
                    image[
                        y + row * scale : y + (row + 1) * scale,
                        cursor + column * scale : cursor + (column + 1) * scale,
                    ] = color
        cursor += (len(glyph[0]) + 2) * scale


def generate_reference_images() -> tuple[Any, Any]:
    """Rasterize fixed non-sensitive simplified/traditional/English samples in memory."""
    try:
        import numpy as np
    except ImportError as error:
        raise ExportError("numpy is required to generate reference images") from error
    detector = np.full((96, 320, 3), 248, dtype=np.uint8)
    detector[30:32, 8:312] = (190, 205, 225)
    detector[62:64, 8:312] = (190, 205, 225)
    _draw_bitmap_text(detector, "安全", x=12, y=6, scale=2, color=(22, 56, 110))
    _draw_bitmap_text(detector, "繁體", x=12, y=38, scale=2, color=(104, 42, 78))
    _draw_bitmap_text(detector, "SAFE", x=12, y=72, scale=2, color=(25, 88, 62))

    recognizer = np.full((48, 320, 3), 248, dtype=np.uint8)
    _draw_bitmap_text(recognizer, "安全", x=8, y=14, scale=2, color=(22, 56, 110))
    _draw_bitmap_text(recognizer, "繁體", x=62, y=14, scale=2, color=(104, 42, 78))
    _draw_bitmap_text(recognizer, "SAFE", x=118, y=16, scale=2, color=(25, 88, 62))
    return detector, recognizer


def _normalize(image: Any, mean: Sequence[float], std: Sequence[float]) -> Any:
    import numpy as np

    scaled = image.astype(np.float32) / np.float32(255.0)
    normalized = (scaled - np.asarray(mean, dtype=np.float32)) / np.asarray(
        std, dtype=np.float32
    )
    return np.ascontiguousarray(normalized.transpose(2, 0, 1)[None, ...])


def reference_tensors() -> tuple[Any, Any]:
    detector_image, recognizer_image = generate_reference_images()
    detector = _normalize(
        detector_image,
        mean=(0.485, 0.456, 0.406),
        std=(0.229, 0.224, 0.225),
    )
    recognizer = _normalize(
        recognizer_image,
        mean=(0.5, 0.5, 0.5),
        std=(0.5, 0.5, 0.5),
    )
    return detector, recognizer


def configure_paddle_reference(config: Any) -> None:
    config.disable_gpu()
    config.disable_glog_info()
    config.set_cpu_math_library_num_threads(1)


def _run_paddle(model_directory: Path, tensor: Any) -> tuple[str, str, Any]:
    try:
        import paddle.inference as paddle_inference
    except ImportError as error:
        raise ExportError("paddlepaddle is required for numerical comparison") from error
    config = paddle_inference.Config(
        os.fspath(model_directory / "inference.json"),
        os.fspath(model_directory / "inference.pdiparams"),
    )
    configure_paddle_reference(config)
    predictor = paddle_inference.create_predictor(config)
    input_names = predictor.get_input_names()
    output_names = predictor.get_output_names()
    if len(input_names) != 1 or len(output_names) != 1:
        raise ExportError("Paddle model must expose one input and one output")
    handle = predictor.get_input_handle(input_names[0])
    handle.reshape(tensor.shape)
    handle.copy_from_cpu(tensor)
    predictor.run()
    return (
        input_names[0],
        output_names[0],
        predictor.get_output_handle(output_names[0]).copy_to_cpu(),
    )


def _run_onnx(path: Path, tensor: Any) -> tuple[str, str, Any]:
    try:
        import onnxruntime
    except ImportError as error:
        raise ExportError("onnxruntime is required for numerical comparison") from error
    options = onnxruntime.SessionOptions()
    options.intra_op_num_threads = 1
    options.graph_optimization_level = (
        onnxruntime.GraphOptimizationLevel.ORT_ENABLE_ALL
    )
    session = onnxruntime.InferenceSession(
        os.fspath(path),
        sess_options=options,
        providers=["CPUExecutionProvider"],
    )
    if len(session.get_inputs()) != 1 or len(session.get_outputs()) != 1:
        raise ExportError("ONNX model must expose one input and one output")
    input_name = session.get_inputs()[0].name
    output_name = session.get_outputs()[0].name
    output = session.run([output_name], {input_name: tensor})[0]
    return input_name, output_name, output


def compare_paddle_and_onnx(
    model_directory: Path, onnx_path: Path, tensor: Any
) -> tuple[str, str, Any]:
    import numpy as np

    paddle_input, paddle_output, expected = _run_paddle(model_directory, tensor)
    onnx_input, onnx_output, actual = _run_onnx(onnx_path, tensor)
    if paddle_input != onnx_input or paddle_output != onnx_output:
        raise ExportError("Paddle and ONNX tensor names differ")
    if expected.shape != actual.shape:
        raise ExportError("Paddle and ONNX output shapes differ")
    if not np.isfinite(expected).all() or not np.isfinite(actual).all():
        raise ExportError("Paddle or ONNX output contains non-finite values")
    if not np.allclose(
        expected,
        actual,
        rtol=PADDLE_COMPARISON_RTOL,
        atol=PADDLE_COMPARISON_ATOL,
    ):
        maximum_difference = float(np.max(np.abs(expected - actual)))
        raise ExportError(
            "Paddle and ONNX outputs exceed cross-backend tolerances "
            f"(maximum absolute difference {maximum_difference:g})"
        )
    return onnx_input, onnx_output, actual


def _dictionary_entries(
    recognizer_metadata: dict[str, Any], class_count: int
) -> list[str]:
    entries = recognizer_metadata.get("PostProcess", {}).get("character_dict")
    if (
        not isinstance(entries, list)
        or not entries
        or any(
            not isinstance(entry, str)
            or not entry
            or "\n" in entry
            or "\r" in entry
            for entry in entries
        )
    ):
        raise ExportError("recognizer character dictionary is invalid")
    result = list(entries)
    if len(result) + 2 == class_count and " " not in result:
        result.append(" ")
    if len(result) + 1 != class_count or len(set(result)) != len(result):
        raise ExportError("recognizer dictionary does not match ONNX class count")
    return result


def _write_bytes(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)


def _asset(kind: str, version: str, metadata: dict[str, int | str]) -> dict[str, Any]:
    return {
        "kind": kind,
        "version": version,
        "license": "Apache-2.0",
        "sha256": metadata["sha256"],
    }


def bundle_digest(directory: Path) -> str:
    entries = {
        relative: artifact_metadata(directory / relative)
        for relative in BUNDLE_DIGEST_PATHS
    }
    canonical = json.dumps(entries, sort_keys=True, separators=(",", ":")).encode(
        "utf-8"
    )
    return hashlib.sha256(canonical).hexdigest()


def _reference_artifact(directory: Path, relative: str) -> dict[str, Any]:
    return {"path": relative, **artifact_metadata(directory / relative)}


def _build_manifest(
    *,
    directory: Path,
    profile: str,
    configuration: ExportConfiguration,
    detector_pin: ModelPin,
    recognizer_pin: ModelPin,
    detector_names: tuple[str, str],
    recognizer_names: tuple[str, str],
    dictionary_entries: list[str],
) -> dict[str, Any]:
    detector_metadata = artifact_metadata(directory / "detector.onnx")
    recognizer_metadata = artifact_metadata(directory / "recognizer.onnx")
    dictionary_metadata = artifact_metadata(directory / "dictionary.txt")
    version = f"pp-ocrv5-{configuration.release}-{profile}"
    return {
        "format_version": 1,
        "asset": {
            "kind": "ocr_bundle",
            "version": version,
            "license": "Apache-2.0",
            "sha256": bundle_digest(directory),
        },
        "profile": profile,
        "source_repository": configuration.repository,
        "source_revision": configuration.revision,
        "detector": {
            "asset": _asset("ocr_detector", version, detector_metadata),
            "model_name": detector_pin.name,
            "upstream": {
                "download_url": detector_pin.url,
                "file_bytes": detector_pin.file_bytes,
                "sha256": detector_pin.sha256,
            },
            "file_name": "detector.onnx",
            "file_bytes": detector_metadata["file_bytes"],
        },
        "recognizer": {
            "asset": _asset("ocr_recognizer", version, recognizer_metadata),
            "model_name": recognizer_pin.name,
            "upstream": {
                "download_url": recognizer_pin.url,
                "file_bytes": recognizer_pin.file_bytes,
                "sha256": recognizer_pin.sha256,
            },
            "file_name": "recognizer.onnx",
            "file_bytes": recognizer_metadata["file_bytes"],
        },
        "dictionary": {
            "asset": _asset("ocr_dictionary", version, dictionary_metadata),
            "file_name": "dictionary.txt",
            "file_bytes": dictionary_metadata["file_bytes"],
            "entries": dictionary_entries,
            "blank_index": 0,
            "languages": [
                "english",
                "chinese_simplified",
                "chinese_traditional",
            ],
        },
        "detector_contract": {
            "input_name": detector_names[0],
            "output_name": detector_names[1],
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
        },
        "recognizer_contract": {
            "input_name": recognizer_names[0],
            "output_name": recognizer_names[1],
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
        },
        "thresholds": {
            "probability": 0.3,
            "text_box": 0.6,
            "expansion": 1.5,
            "recognition_confidence": 0.5,
        },
        "resource_limits": {
            "maximum_text_boxes": 64,
            "minimum_box_side_pixels": 6,
            "minimum_box_area_pixels": 48,
            "recognizer_height": 48,
            "maximum_recognizer_width": 320,
            "maximum_batch_size": 8,
            "maximum_line_characters": 128,
            "maximum_total_characters": 4096,
        },
        "reference_artifacts": {
            "detector_input": _reference_artifact(
                directory, "reference/detector-input.bin"
            ),
            "detector_output": _reference_artifact(
                directory, "reference/detector-output.json"
            ),
            "recognizer_input": _reference_artifact(
                directory, "reference/recognizer-input.bin"
            ),
            "recognizer_output": _reference_artifact(
                directory, "reference/recognizer-output.json"
            ),
        },
        "export_toolchain": {
            key: configuration.toolchain[key]
            for key in TOOLCHAIN_DISTRIBUTIONS
        },
        "opset": OPSET,
        "minimum_runtime_version": "1.22",
    }


def _write_manifest(path: Path, manifest: dict[str, Any]) -> None:
    path.write_text(
        json.dumps(
            manifest,
            allow_nan=False,
            ensure_ascii=False,
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


def export_bundle(profile: str, output: Path) -> None:
    configuration = load_model_config()
    validate_installed_toolchain(configuration)
    profile_models = configuration.profiles[profile]
    detector_pin = configuration.models[profile_models["detector"]]
    recognizer_pin = configuration.models[profile_models["recognizer"]]
    output = output.expanduser().resolve()
    if output.exists():
        raise ExportError("output directory already exists")
    output.parent.mkdir(parents=True, exist_ok=True)
    cache = output.parent / ".cache" / "ocr-export"

    detector_archive = download_model(detector_pin, cache)
    recognizer_archive = download_model(recognizer_pin, cache)
    with tempfile.TemporaryDirectory(
        prefix=".ocr-export-work-", dir=output.parent
    ) as temporary:
        work = Path(temporary)
        extracted = work / "upstream"
        detector_extract = extracted / "detector"
        recognizer_extract = extracted / "recognizer"
        safe_extract_tar(detector_archive, detector_extract)
        safe_extract_tar(recognizer_archive, recognizer_extract)
        detector_directory = detector_extract / f"{detector_pin.name}_infer"
        recognizer_directory = recognizer_extract / f"{recognizer_pin.name}_infer"
        _validate_model_directory(detector_directory, detector_pin)
        recognizer_metadata = _validate_model_directory(
            recognizer_directory, recognizer_pin
        )

        bundle = work / "bundle"
        bundle.mkdir()
        for attribution in ("LICENSE", "NOTICE.md"):
            shutil.copyfile(ATTRIBUTION_DIRECTORY / attribution, bundle / attribution)
        converter = _paddle2onnx_executable()
        run_checked(
            build_converter_command(
                converter, detector_directory, bundle / "detector.onnx"
            )
        )
        run_checked(
            build_converter_command(
                converter, recognizer_directory, bundle / "recognizer.onnx"
            )
        )
        constrain_detector_batch(bundle / "detector.onnx")
        clamp_detector_probability_output(bundle / "detector.onnx")
        detector_contract = check_onnx_contract(bundle / "detector.onnx", "detector")
        recognizer_contract = check_onnx_contract(
            bundle / "recognizer.onnx", "recognizer"
        )
        class_count = recognizer_contract[2]
        if class_count is None:
            raise ExportError("recognizer class count is not static")
        dictionary_entries = _dictionary_entries(recognizer_metadata, class_count)
        (bundle / "dictionary.txt").write_text(
            "\n".join(dictionary_entries) + "\n", encoding="utf-8"
        )

        detector_input, recognizer_input = reference_tensors()
        detector_names = compare_paddle_and_onnx(
            detector_directory,
            bundle / "detector.onnx",
            detector_input,
        )
        recognizer_names = compare_paddle_and_onnx(
            recognizer_directory,
            bundle / "recognizer.onnx",
            recognizer_input,
        )
        if detector_names[:2] != detector_contract[:2]:
            raise ExportError("detector runtime names do not match its graph")
        if recognizer_names[:2] != recognizer_contract[:2]:
            raise ExportError("recognizer runtime names do not match its graph")

        _write_bytes(
            bundle / "reference/detector-input.bin",
            encode_reference_tensor(
                detector_input.shape, detector_input.reshape(-1).tolist()
            ),
        )
        _write_bytes(
            bundle / "reference/detector-output.json",
            encode_reference_output(
                detector_names[2].shape, detector_names[2].reshape(-1).tolist()
            ),
        )
        _write_bytes(
            bundle / "reference/recognizer-input.bin",
            encode_reference_tensor(
                recognizer_input.shape, recognizer_input.reshape(-1).tolist()
            ),
        )
        _write_bytes(
            bundle / "reference/recognizer-output.json",
            encode_reference_output(
                recognizer_names[2].shape, recognizer_names[2].reshape(-1).tolist()
            ),
        )

        manifest = _build_manifest(
            directory=bundle,
            profile=profile,
            configuration=configuration,
            detector_pin=detector_pin,
            recognizer_pin=recognizer_pin,
            detector_names=detector_names[:2],
            recognizer_names=recognizer_names[:2],
            dictionary_entries=dictionary_entries,
        )
        _write_manifest(bundle / "manifest.json", manifest)
        run_checked(
            [
                sys.executable,
                TOOL_DIRECTORY / "verify.py",
                bundle / "manifest.json",
            ]
        )
        bundle.replace(output)
    print(
        f"status=exported profile={profile} "
        f"revision={configuration.revision} output={output}"
    )


def build_argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Export a pinned official PP-OCRv5 bundle to ONNX."
    )
    parser.add_argument(
        "--profile",
        choices=("lightweight", "accurate"),
        required=True,
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="New output directory; existing paths are never overwritten.",
    )
    return parser


def main() -> int:
    try:
        require_production_python()
        arguments = build_argument_parser().parse_args()
        export_bundle(arguments.profile, arguments.output)
    except (ExportError, OSError, subprocess.CalledProcessError) as error:
        print(f"status=failed error={type(error).__name__}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
