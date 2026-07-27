from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from types import SimpleNamespace
import unittest
from unittest import mock


TOOL_DIRECTORY = Path(__file__).resolve().parents[1]
EXPORT_PATH = TOOL_DIRECTORY / "export.py"
MODELS_PATH = TOOL_DIRECTORY / "models.toml"
REQUIREMENTS_PATH = TOOL_DIRECTORY / "requirements.lock"


def load_export_module():
    spec = importlib.util.spec_from_file_location("karma_ocr_export", EXPORT_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("unable to load exporter")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_verify_module(exporter):
    verify_path = TOOL_DIRECTORY / "verify.py"
    spec = importlib.util.spec_from_file_location("karma_ocr_verify", verify_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("unable to load verifier")
    module = importlib.util.module_from_spec(spec)
    with mock.patch.dict(
        sys.modules,
        {spec.name: module, "export": exporter},
    ):
        spec.loader.exec_module(module)
    return module


class ConfigurationContractTests(unittest.TestCase):
    def test_models_pin_an_immutable_official_revision_and_all_four_archives(self):
        config = tomllib.loads(MODELS_PATH.read_text(encoding="utf-8"))
        provenance = config["provenance"]
        self.assertEqual(
            provenance["repository"], "https://github.com/PaddlePaddle/PaddleOCR"
        )
        self.assertEqual(
            provenance["revision"], "33cbdd9deb2e00f61e7966db70669b249c005a37"
        )
        self.assertEqual(len(provenance["revision"]), 40)

        expected = {
            "PP-OCRv5_mobile_det": (
                4_935_680,
                "50446e5d01ac2a73d5319c89513281f6578414c888c602f9af13f93feefffc58",
            ),
            "PP-OCRv5_mobile_rec": (
                16_834_560,
                "566b9512b34e34a9f0db54d87b51fa5a0b9ed2cf1ab7e49728cc0b8b5a64f414",
            ),
            "PP-OCRv5_server_det": (
                88_340_480,
                "22a33e0ba6a21425ea4192da03bf4395c9a0c67902bd924b7328fc859073045d",
            ),
            "PP-OCRv5_server_rec": (
                84_869_120,
                "d99be2ffd348943ab52876179168be4fb5b14f5f0812f2ae4c76d89ec2ea750a",
            ),
        }
        self.assertEqual(set(config["models"]), set(expected))
        for model_name, (expected_bytes, expected_sha256) in expected.items():
            model = config["models"][model_name]
            self.assertEqual(
                model["url"],
                "https://paddle-model-ecology.bj.bcebos.com/"
                "paddlex/official_inference_model/paddle3.0.0/"
                f"{model_name}_infer.tar",
            )
            self.assertEqual(model["file_bytes"], expected_bytes)
            self.assertEqual(model["sha256"], expected_sha256)

        self.assertEqual(
            config["profiles"]["lightweight"],
            {
                "detector": "PP-OCRv5_mobile_det",
                "recognizer": "PP-OCRv5_mobile_rec",
            },
        )
        self.assertEqual(
            config["profiles"]["accurate"],
            {
                "detector": "PP-OCRv5_server_det",
                "recognizer": "PP-OCRv5_server_rec",
            },
        )
        self.assertEqual(
            config["toolchain"],
            {
                "python_implementation": "cpython",
                "python_version": "3.11",
                "paddlepaddle_version": "3.0.0",
                "paddle2onnx_version": "2.1.0",
                "onnx_version": "1.17.0",
                "onnx_runtime_version": "1.22.0",
            },
        )

    def test_requirements_are_exact_and_hashed(self):
        lines = REQUIREMENTS_PATH.read_text(encoding="utf-8").splitlines()
        packages = [
            line
            for line in lines
            if line and not line.startswith(("#", " ", "\t", "--"))
        ]
        self.assertGreaterEqual(len(packages), 8)
        for package in packages:
            self.assertIn("==", package)
        text = "\n".join(lines)
        self.assertNotIn("git+", text)
        package_indexes = [
            index
            for index, line in enumerate(lines)
            if line and not line.startswith(("#", " ", "\t", "--"))
        ]
        for offset, index in enumerate(package_indexes):
            end = (
                package_indexes[offset + 1]
                if offset + 1 < len(package_indexes)
                else len(lines)
            )
            self.assertTrue(
                any(
                    line.lstrip().startswith("--hash=sha256:")
                    for line in lines[index:end]
                ),
                msg=f"{lines[index]} has no SHA-256",
            )


class SafeExtractionTests(unittest.TestCase):
    @staticmethod
    def write_archive(path: Path, members: list[tuple[tarfile.TarInfo, bytes]]) -> None:
        with tarfile.open(path, "w") as archive:
            for info, payload in members:
                if info.isreg():
                    info.size = len(payload)
                    archive.addfile(info, io.BytesIO(payload))
                else:
                    archive.addfile(info)

    def test_extracts_only_regular_files_and_directories(self):
        exporter = load_export_module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "valid.tar"
            directory = tarfile.TarInfo("model")
            directory.type = tarfile.DIRTYPE
            model = tarfile.TarInfo("model/inference.json")
            self.write_archive(archive, [(directory, b""), (model, b"model")])

            output = root / "output"
            exporter.safe_extract_tar(archive, output)

            self.assertEqual((output / "model/inference.json").read_bytes(), b"model")

    def test_rejects_traversal_links_devices_and_duplicates_before_writing(self):
        exporter = load_export_module()
        bad_members = []

        traversal = tarfile.TarInfo("../escape")
        bad_members.append([traversal])

        windows_traversal = tarfile.TarInfo(r"..\escape")
        bad_members.append([windows_traversal])

        symlink = tarfile.TarInfo("model/link")
        symlink.type = tarfile.SYMTYPE
        symlink.linkname = "../escape"
        bad_members.append([symlink])

        hardlink = tarfile.TarInfo("model/hardlink")
        hardlink.type = tarfile.LNKTYPE
        hardlink.linkname = "model/file"
        bad_members.append([hardlink])

        device = tarfile.TarInfo("model/device")
        device.type = tarfile.CHRTYPE
        bad_members.append([device])

        first = tarfile.TarInfo("model/inference.json")
        second = tarfile.TarInfo("model/inference.json")
        bad_members.append([first, second])

        file_parent = tarfile.TarInfo("model")
        nested_file = tarfile.TarInfo("model/inference.json")
        bad_members.append([file_parent, nested_file])

        for index, infos in enumerate(bad_members):
            with self.subTest(index=index), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = root / "bad.tar"
                self.write_archive(archive, [(info, b"x") for info in infos])
                output = root / "output"

                with self.assertRaises(exporter.ExportError):
                    exporter.safe_extract_tar(archive, output)

                self.assertFalse(output.exists())
                self.assertFalse((root / "escape").exists())

    def test_rejects_old_gnu_sparse_before_writing_any_output(self):
        exporter = load_export_module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "old-gnu-sparse.tar"
            sparse = tarfile.TarInfo("model/inference.pdiparams")
            sparse.type = tarfile.GNUTYPE_SPARSE
            sparse.size = 0
            archive.write_bytes(
                sparse.tobuf(format=tarfile.GNU_FORMAT) + b"\0" * 1024
            )

            with tarfile.open(archive, mode="r:") as value:
                member = value.getmembers()[0]
                self.assertTrue(member.isreg())
                self.assertTrue(member.issparse())
                self.assertIsNotNone(member.sparse)

            output = root / "output"
            with self.assertRaises(exporter.ExportError):
                exporter.safe_extract_tar(archive, output)
            self.assertFalse(output.exists())


class ExportPrimitiveTests(unittest.TestCase):
    def test_failed_subprocess_is_never_accepted(self):
        exporter = load_export_module()
        with self.assertRaises(subprocess.CalledProcessError):
            exporter.run_checked([sys.executable, "-c", "raise SystemExit(7)"])

    def test_artifact_metadata_records_exact_length_and_sha256(self):
        exporter = load_export_module()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "artifact"
            path.write_bytes(b"official bytes")

            self.assertEqual(
                exporter.artifact_metadata(path),
                {
                    "file_bytes": 14,
                    "sha256": hashlib.sha256(b"official bytes").hexdigest(),
                },
            )

    def test_reference_formats_match_the_runtime_contract_exactly(self):
        exporter = load_export_module()
        binary = exporter.encode_reference_tensor([1, 2], [1.5, -2.25])
        self.assertEqual(binary[:4], b"KOR1")
        self.assertEqual(struct.unpack("<I", binary[4:8]), (2,))
        self.assertEqual(struct.unpack("<II", binary[8:16]), (1, 2))
        self.assertEqual(struct.unpack("<ff", binary[16:]), (1.5, -2.25))

        output = exporter.encode_reference_output([1, 2], [1.5, -2.25])
        decoded = json.loads(output)
        self.assertEqual(set(decoded), {"shape", "values"})
        self.assertEqual(decoded, {"shape": [1, 2], "values": [1.5, -2.25]})

    def test_converter_command_is_checked_dynamic_opset_18_export(self):
        exporter = load_export_module()
        command = exporter.build_converter_command(
            Path("/venv/bin/paddle2onnx"),
            Path("/models/PP-OCRv5_mobile_det_infer"),
            Path("/output/detector.onnx"),
        )
        self.assertIn("--model_filename", command)
        self.assertIn("inference.json", command)
        self.assertIn("--params_filename", command)
        self.assertIn("inference.pdiparams", command)
        self.assertEqual(command[command.index("--opset_version") + 1], "18")
        self.assertEqual(command[command.index("--enable_onnx_checker") + 1], "True")
        self.assertEqual(command[command.index("--optimize_tool") + 1], "None")

    def test_converter_discovery_stays_in_the_active_venv(self):
        exporter = load_export_module()
        with tempfile.TemporaryDirectory() as temporary:
            prefix = Path(temporary) / "venv"
            binary_directory = prefix / ("Scripts" if os.name == "nt" else "bin")
            binary_directory.mkdir(parents=True)
            converter_name = "paddle2onnx.exe" if os.name == "nt" else "paddle2onnx"
            converter = binary_directory / converter_name
            converter.write_text("#!/bin/sh\n", encoding="utf-8")
            converter.chmod(0o755)

            with (
                mock.patch.object(exporter.sys, "prefix", str(prefix)),
                mock.patch.object(
                    exporter.sys,
                    "base_prefix",
                    str(base_prefix := Path(temporary) / "base"),
                ),
            ):
                self.assertEqual(
                    exporter._paddle2onnx_executable(),
                    converter.resolve(),
                )
                self.assertNotEqual(prefix, base_prefix)

    def test_converter_discovery_never_falls_back_to_path_or_symlinks(self):
        exporter = load_export_module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            prefix = root / "venv"
            binary_directory = prefix / ("Scripts" if os.name == "nt" else "bin")
            binary_directory.mkdir(parents=True)
            path_binary = root / "path-bin"
            path_binary.mkdir()
            converter_name = "paddle2onnx.exe" if os.name == "nt" else "paddle2onnx"
            outside = path_binary / converter_name
            outside.write_text("#!/bin/sh\n", encoding="utf-8")
            outside.chmod(0o755)
            (binary_directory / converter_name).symlink_to(outside)

            with (
                mock.patch.object(exporter.sys, "prefix", str(prefix)),
                mock.patch.object(exporter.sys, "base_prefix", str(root / "base")),
                mock.patch.dict(os.environ, {"PATH": str(path_binary)}),
                self.assertRaises(exporter.ExportError),
            ):
                exporter._paddle2onnx_executable()

    def test_production_cli_requires_exact_cpython_311(self):
        exporter = load_export_module()
        with (
            mock.patch.object(
                exporter.sys, "implementation", SimpleNamespace(name="pypy")
            ),
            self.assertRaises(exporter.ExportError),
        ):
            exporter.require_production_python()
        with (
            mock.patch.object(exporter.sys, "version_info", (3, 12, 0)),
            self.assertRaises(exporter.ExportError),
        ):
            exporter.require_production_python()

    def test_export_and_verify_main_fail_before_argument_work_on_wrong_python(self):
        exporter = load_export_module()
        verifier = load_verify_module(exporter)
        for module in (exporter, verifier):
            with (
                self.subTest(module=module.__name__),
                mock.patch.object(
                    exporter,
                    "require_production_python",
                    side_effect=exporter.ExportError("wrong interpreter"),
                ),
                mock.patch.object(
                    module,
                    "build_argument_parser",
                    side_effect=AssertionError("argument work must not begin"),
                ),
            ):
                self.assertEqual(module.main(), 1)

    def test_installed_toolchain_must_match_reviewed_versions(self):
        exporter = load_export_module()
        configuration = exporter.load_model_config()
        actual = {
            "paddlepaddle": "3.0.0",
            "paddle2onnx": "2.1.0",
            "onnx": "1.17.0",
            "onnxruntime": "1.22.1",
        }
        with (
            mock.patch.object(
                exporter.importlib.metadata,
                "version",
                side_effect=lambda name: actual[name],
            ),
            self.assertRaises(exporter.ExportError),
        ):
            exporter.validate_installed_toolchain(configuration)

    def test_detector_keeps_spatial_axes_dynamic_but_fixes_single_batch(self):
        exporter = load_export_module()
        self.assertEqual(
            exporter.detector_shape_contract(
                [None, 3, None, None], [None, 1, None, None]
            ),
            ([1, 3, None, None], [1, 1, None, None]),
        )
        with self.assertRaises(exporter.ExportError):
            exporter.detector_shape_contract(
                [None, 3, 640, None], [None, 1, 640, None]
            )

    def test_detector_export_clamps_probability_roundoff(self):
        exporter = load_export_module()
        producer = SimpleNamespace(
            name="Sigmoid.0",
            op_type="Sigmoid",
            input=["logits"],
            output=["fetch_name_0"],
        )
        graph = SimpleNamespace(
            output=[SimpleNamespace(name="fetch_name_0")],
            node=[producer],
            initializer=[],
        )
        model = SimpleNamespace(
            graph=graph,
            SerializeToString=lambda deterministic: b"deterministic-model",
        )

        def make_tensor(name, data_type, dims, values):
            return SimpleNamespace(
                name=name, data_type=data_type, dims=dims, values=values
            )

        def make_node(op_type, inputs, outputs, name):
            return SimpleNamespace(
                name=name,
                op_type=op_type,
                input=inputs,
                output=outputs,
            )

        fake_onnx = SimpleNamespace(
            load=lambda path: model,
            checker=SimpleNamespace(check_model=lambda value: None),
            helper=SimpleNamespace(make_node=make_node, make_tensor=make_tensor),
            TensorProto=SimpleNamespace(FLOAT=1),
        )
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "detector.onnx"
            with mock.patch.dict(sys.modules, {"onnx": fake_onnx}):
                exporter.clamp_detector_probability_output(path)

            self.assertEqual(path.read_bytes(), b"deterministic-model")
        self.assertEqual(producer.output, ["fetch_name_0.unclamped"])
        self.assertEqual(
            [(node.op_type, node.input, node.output) for node in graph.node[-1:]],
            [
                (
                    "Clip",
                    [
                        "fetch_name_0.unclamped",
                        "karma.detector.probability.minimum",
                        "karma.detector.probability.maximum",
                    ],
                    ["fetch_name_0"],
                )
            ],
        )
        self.assertEqual(
            [(value.name, value.values) for value in graph.initializer],
            [
                ("karma.detector.probability.minimum", [0.0]),
                ("karma.detector.probability.maximum", [1.0]),
            ],
        )

    def test_onnx_contract_rejects_non_f32_detector_and_recognizer(self):
        exporter = load_export_module()
        try:
            import onnx
        except ImportError as error:
            self.skipTest(f"pinned ONNX dependency unavailable: {error}")

        def non_f32_model(role: str):
            if role == "detector":
                input_shape = [1, 3, "height", "width"]
                output_shape = [1, 1, "height", "width"]
                input_info = onnx.helper.make_tensor_value_info(
                    "x", onnx.TensorProto.DOUBLE, input_shape
                )
                output_info = onnx.helper.make_tensor_value_info(
                    "fetch_name_0", onnx.TensorProto.DOUBLE, output_shape
                )
                nodes = [
                    onnx.helper.make_node(
                        "Identity", ["x"], ["fetch_name_0.unclamped"]
                    ),
                    onnx.helper.make_node(
                        "Clip",
                        [
                            "fetch_name_0.unclamped",
                            "karma.detector.probability.minimum",
                            "karma.detector.probability.maximum",
                        ],
                        ["fetch_name_0"],
                    ),
                ]
                initializers = [
                    onnx.helper.make_tensor(
                        "karma.detector.probability.minimum",
                        onnx.TensorProto.DOUBLE,
                        [],
                        [0.0],
                    ),
                    onnx.helper.make_tensor(
                        "karma.detector.probability.maximum",
                        onnx.TensorProto.DOUBLE,
                        [],
                        [1.0],
                    ),
                ]
            else:
                input_info = onnx.helper.make_tensor_value_info(
                    "x",
                    onnx.TensorProto.DOUBLE,
                    ["batch", 3, 48, "width"],
                )
                output_info = onnx.helper.make_tensor_value_info(
                    "fetch_name_0",
                    onnx.TensorProto.DOUBLE,
                    ["batch", "time", 2],
                )
                nodes = [
                    onnx.helper.make_node("Identity", ["x"], ["fetch_name_0"])
                ]
                initializers = []
            graph = onnx.helper.make_graph(
                nodes,
                f"non-f32-{role}",
                [input_info],
                [output_info],
                initializer=initializers,
            )
            return onnx.helper.make_model(
                graph,
                opset_imports=[onnx.helper.make_opsetid("", 18)],
            )

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for role in ("detector", "recognizer"):
                with self.subTest(role=role):
                    path = root / f"{role}.onnx"
                    onnx.save(non_f32_model(role), path)
                    with self.assertRaises(exporter.ExportError):
                        exporter.check_onnx_contract(path, role)

    def test_paddle_reference_configuration_avoids_legacy_memory_pass(self):
        exporter = load_export_module()

        class ConfigurationProbe:
            def __init__(self):
                self.calls = []

            def disable_gpu(self):
                self.calls.append("disable_gpu")

            def disable_glog_info(self):
                self.calls.append("disable_glog_info")

            def set_cpu_math_library_num_threads(self, threads):
                self.calls.append(("threads", threads))

            def enable_memory_optim(self):
                raise AssertionError("legacy PIR-incompatible pass must not be enabled")

        probe = ConfigurationProbe()
        exporter.configure_paddle_reference(probe)
        self.assertEqual(
            probe.calls,
            ["disable_gpu", "disable_glog_info", ("threads", 1)],
        )

    def test_cross_backend_tolerance_does_not_weaken_runtime_references(self):
        exporter = load_export_module()
        self.assertEqual(exporter.REFERENCE_TOLERANCE, 1.0e-4)
        self.assertEqual(exporter.PADDLE_COMPARISON_ATOL, 1.0e-4)
        self.assertEqual(exporter.PADDLE_COMPARISON_RTOL, 3.0e-4)

    def test_output_directory_is_required(self):
        exporter = load_export_module()
        with self.assertRaises(SystemExit):
            exporter.build_argument_parser().parse_args(["--profile", "lightweight"])


class VerifierHardeningTests(unittest.TestCase):
    @staticmethod
    def exact_tree(root: Path) -> None:
        exporter = load_export_module()
        for relative in {"manifest.json", *exporter.BUNDLE_DIGEST_PATHS}:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"x")

    def test_verifier_toolchain_cannot_self_attest_a_mismatched_environment(self):
        exporter = load_export_module()
        verifier = load_verify_module(exporter)
        configuration = exporter.load_model_config()
        mismatched = {
            "paddlepaddle_version": "3.0.0",
            "paddle2onnx_version": "2.1.0",
            "onnx_version": "1.17.0",
            "onnx_runtime_version": "1.22.1",
        }
        with (
            mock.patch.object(
                exporter,
                "_package_version",
                side_effect=lambda name: {
                    "paddlepaddle": "3.0.0",
                    "paddle2onnx": "2.1.0",
                    "onnx": "1.17.0",
                    "onnxruntime": "1.22.1",
                }[name],
            ),
            self.assertRaises(exporter.ExportError),
        ):
            verifier._verify_toolchain(mismatched, configuration)

    def test_verifier_stats_oversized_files_before_reading_them(self):
        exporter = load_export_module()
        verifier = load_verify_module(exporter)
        self.assertEqual(verifier.MAXIMUM_MANIFEST_BYTES, 1024 * 1024)
        self.assertEqual(verifier.MAXIMUM_MODEL_BYTES, 256 * 1024 * 1024)
        self.assertEqual(verifier.MAXIMUM_DICTIONARY_BYTES, 4 * 1024 * 1024)
        self.assertEqual(verifier.MAXIMUM_LICENSE_BYTES, 64 * 1024)
        self.assertEqual(verifier.MAXIMUM_REFERENCE_BYTES, 256 * 1024 * 1024)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "manifest.json"
            with manifest.open("wb") as output:
                output.truncate(verifier.MAXIMUM_MANIFEST_BYTES + 1)
            reference = root / "reference.bin"
            with reference.open("wb") as output:
                output.truncate(verifier.MAXIMUM_REFERENCE_BYTES + 1)

            with (
                mock.patch.object(
                    Path,
                    "read_bytes",
                    side_effect=AssertionError("unbounded read attempted"),
                ),
                self.assertRaises(exporter.ExportError),
            ):
                verifier._read_manifest(manifest)
            with (
                mock.patch.object(
                    Path,
                    "read_bytes",
                    side_effect=AssertionError("unbounded read attempted"),
                ),
                self.assertRaises(exporter.ExportError),
            ):
                verifier._parse_reference_input(reference)

    def test_declared_oversized_model_is_rejected_before_hash_or_onnx(self):
        exporter = load_export_module()
        verifier = load_verify_module(exporter)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with (
                mock.patch.object(
                    Path,
                    "open",
                    side_effect=AssertionError("file I/O must not begin"),
                ),
                self.assertRaises(exporter.ExportError),
            ):
                verifier._verify_artifact(
                    root,
                    "detector.onnx",
                    expected_bytes=verifier.MAXIMUM_MODEL_BYTES + 1,
                    expected_sha256="0" * 64,
                    maximum_bytes=verifier.MAXIMUM_MODEL_BYTES,
                )

    def test_exact_tree_rejects_unexpected_directories_and_non_regular_files(self):
        exporter = load_export_module()
        verifier = load_verify_module(exporter)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.exact_tree(root)
            verifier._verify_exact_tree(root)
            (root / "unexpected").mkdir()
            with self.assertRaises(exporter.ExportError):
                verifier._verify_exact_tree(root)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.exact_tree(root)
            notice = root / "NOTICE.md"
            notice.unlink()
            notice.symlink_to(root / "LICENSE")
            with self.assertRaises(exporter.ExportError):
                verifier._verify_exact_tree(root)

        if hasattr(os, "mkfifo"):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.exact_tree(root)
                notice = root / "NOTICE.md"
                notice.unlink()
                os.mkfifo(notice)
                with self.assertRaises(exporter.ExportError):
                    verifier._verify_exact_tree(root)

        if hasattr(socket, "AF_UNIX"):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.exact_tree(root)
                notice = root / "NOTICE.md"
                notice.unlink()
                value = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                try:
                    try:
                        value.bind(os.fspath(notice))
                    except PermissionError:
                        pass
                    else:
                        with self.assertRaises(exporter.ExportError):
                            verifier._verify_exact_tree(root)
                finally:
                    value.close()


if __name__ == "__main__":
    unittest.main()
