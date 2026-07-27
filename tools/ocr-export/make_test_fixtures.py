#!/usr/bin/env python3
"""Generate tiny deterministic ONNX graphs for OCR runtime contract tests."""

from pathlib import Path

import onnx
from onnx import TensorProto, helper, numpy_helper
import numpy as np


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "crates" / "karma-onnx" / "tests" / "fixtures"
OPSET = 18
MAX_FIXTURE_BYTES = 50 * 1024


def model(graph: onnx.GraphProto) -> onnx.ModelProto:
    value = helper.make_model(
        graph,
        producer_name="karma-test-fixtures",
        opset_imports=[helper.make_opsetid("", OPSET)],
    )
    value.ir_version = 10
    onnx.checker.check_model(value)
    return value


def detector() -> onnx.ModelProto:
    input_value = helper.make_tensor_value_info(
        "x", TensorProto.FLOAT, [1, 3, "height", "width"]
    )
    output_value = helper.make_tensor_value_info(
        "sigmoid_0.tmp_0", TensorProto.FLOAT, [1, 1, "height", "width"]
    )
    axes = numpy_helper.from_array(np.asarray([1], dtype=np.int64), "channel_axis")
    node = helper.make_node(
        "ReduceMean",
        ["x", "channel_axis"],
        ["sigmoid_0.tmp_0"],
        keepdims=1,
    )
    return model(
        helper.make_graph(
            [node],
            "karma_dynamic_ocr_detector",
            [input_value],
            [output_value],
            [axes],
        )
    )


def recognizer() -> onnx.ModelProto:
    input_value = helper.make_tensor_value_info(
        "x", TensorProto.FLOAT, ["batch", 3, 48, "width"]
    )
    output_value = helper.make_tensor_value_info(
        "softmax_0.tmp_0", TensorProto.FLOAT, ["batch", 2, 3]
    )
    template = numpy_helper.from_array(
        np.asarray([[[0.0, 10.0, 0.0], [10.0, 0.0, 0.0]]], dtype=np.float32),
        "fixture_logits",
    )
    batch_index = numpy_helper.from_array(np.asarray(0, dtype=np.int64), "batch_index")
    unsqueeze_axis = numpy_helper.from_array(
        np.asarray([0], dtype=np.int64), "unsqueeze_axis"
    )
    trailing_shape = numpy_helper.from_array(
        np.asarray([2, 3], dtype=np.int64), "trailing_shape"
    )
    nodes = [
        helper.make_node("Shape", ["x"], ["input_shape"]),
        helper.make_node("Gather", ["input_shape", "batch_index"], ["batch_size"]),
        helper.make_node(
            "Unsqueeze", ["batch_size", "unsqueeze_axis"], ["batch_vector"]
        ),
        helper.make_node(
            "Concat",
            ["batch_vector", "trailing_shape"],
            ["output_shape"],
            axis=0,
        ),
        helper.make_node(
            "Expand", ["fixture_logits", "output_shape"], ["softmax_0.tmp_0"]
        ),
    ]
    return model(
        helper.make_graph(
            nodes,
            "karma_dynamic_ocr_recognizer",
            [input_value],
            [output_value],
            [template, batch_index, unsqueeze_axis, trailing_shape],
        )
    )


def write(name: str, value: onnx.ModelProto) -> None:
    path = FIXTURES / name
    payload = value.SerializeToString(deterministic=True)
    if len(payload) >= MAX_FIXTURE_BYTES:
        raise RuntimeError(f"{name} is not a tiny fixture")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    onnx.checker.check_model(onnx.load(path))
    print(f"{path.relative_to(ROOT)}: {len(payload)} bytes")


def main() -> None:
    write("ocr_detector.onnx", detector())
    write("ocr_recognizer.onnx", recognizer())


if __name__ == "__main__":
    main()
