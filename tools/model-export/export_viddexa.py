#!/usr/bin/env python3

import argparse
import hashlib
import json
from pathlib import Path

REPOSITORY = "viddexa/nsfw-detection-2-nano"
SOURCE_REPOSITORY = f"https://huggingface.co/{REPOSITORY}"
REVISION = "913bc502e69fa3edfe2cfce72c98cad4ddc6149b"
PRODUCT_LABELS = ["normal", "hentai", "porn", "sexy", "drawing"]
UPSTREAM_LABELS = ["safe", "hentai", "porn", "sexy", "drawing"]
IMAGE_MEAN = [0.485, 0.456, 0.406]
IMAGE_STD = [0.47853944, 0.4732864, 0.47434163]
RESCALE_FACTOR = 1.0 / 255.0
OPSET = 18


def validate_source(repository: str, revision: str) -> None:
    if repository != REPOSITORY or revision != REVISION:
        raise ValueError("model source must match the pinned repository and revision")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(64 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def build_manifest(
    model_path: Path,
    *,
    input_name: str,
    output_name: str,
    labels: list[str],
) -> dict:
    validate_source(REPOSITORY, REVISION)
    if labels != PRODUCT_LABELS:
        raise ValueError("exported labels do not match the product contract")
    return {
        "asset": {
            "kind": "image_classifier",
            "version": "viddexa-nano-1",
            "license": "Apache-2.0",
            "sha256": sha256_file(model_path),
        },
        "source_repository": SOURCE_REPOSITORY,
        "source_revision": REVISION,
        "file_name": model_path.name,
        "file_bytes": model_path.stat().st_size,
        "opset": OPSET,
        "minimum_runtime_version": "1.22",
        "input": {
            "name": input_name,
            "shape": [1, 3, 224, 224],
            "layout": "nchw",
            "color_order": "rgb",
            "scale": RESCALE_FACTOR,
            "mean": IMAGE_MEAN,
            "std": IMAGE_STD,
        },
        "output_name": output_name,
        "labels": [
            {"index": index, "name": name}
            for index, name in enumerate(labels)
        ],
    }


def _validate_processor(processor) -> None:
    size = getattr(processor, "size", {})
    if size.get("height") != 224 or size.get("width") != 224:
        raise ValueError("upstream processor size changed")
    if list(processor.image_mean) != IMAGE_MEAN:
        raise ValueError("upstream image mean changed")
    if list(processor.image_std) != IMAGE_STD:
        raise ValueError("upstream image standard deviation changed")
    if float(processor.rescale_factor) != RESCALE_FACTOR:
        raise ValueError("upstream rescale factor changed")
    if bool(getattr(processor, "do_center_crop", False)):
        raise ValueError("upstream processor unexpectedly enables center crop")


def _ordered_upstream_labels(config) -> list[str]:
    labels = [str(config.id2label[index]).lower() for index in range(5)]
    if labels != UPSTREAM_LABELS:
        raise ValueError("upstream label order changed")
    return PRODUCT_LABELS


def export_model(output_directory: Path) -> None:
    import numpy as np
    import onnx
    import onnxruntime
    import torch
    from transformers import (
        AutoImageProcessor,
        AutoModelForImageClassification,
    )

    validate_source(REPOSITORY, REVISION)
    output_directory.mkdir(parents=True, exist_ok=True)
    model_path = output_directory / "model.onnx"
    processor = AutoImageProcessor.from_pretrained(
        REPOSITORY,
        revision=REVISION,
        trust_remote_code=False,
        use_fast=False,
    )
    _validate_processor(processor)
    upstream = AutoModelForImageClassification.from_pretrained(
        REPOSITORY,
        revision=REVISION,
        trust_remote_code=False,
        use_safetensors=True,
    )
    labels = _ordered_upstream_labels(upstream.config)
    upstream.eval()

    class LogitsOnly(torch.nn.Module):
        def __init__(self, model):
            super().__init__()
            self.model = model

        def forward(self, pixel_values):
            return self.model(pixel_values=pixel_values).logits

    wrapped = LogitsOnly(upstream).eval()
    raw = torch.arange(3 * 224 * 224, dtype=torch.float32)
    raw = raw.remainder(256).reshape(1, 3, 224, 224) * RESCALE_FACTOR
    mean = torch.tensor(IMAGE_MEAN).reshape(1, 3, 1, 1)
    std = torch.tensor(IMAGE_STD).reshape(1, 3, 1, 1)
    sample = (raw - mean) / std

    with torch.no_grad():
        upstream_logits = upstream(pixel_values=sample).logits.detach().cpu().numpy()
    pooler = upstream.efficientnet.pooler
    if not isinstance(pooler, torch.nn.AvgPool2d):
        raise ValueError("upstream pooler type changed")
    if pooler.kernel_size != 1280 or not pooler.ceil_mode:
        raise ValueError("upstream pooler contract changed")
    upstream.efficientnet.pooler = torch.nn.AdaptiveAvgPool2d(1)
    with torch.no_grad():
        reference_logits = wrapped(sample).detach().cpu().numpy()
    np.testing.assert_allclose(
        reference_logits,
        upstream_logits,
        rtol=0.0,
        atol=1e-5,
    )
    torch.onnx.export(
        wrapped,
        (sample,),
        model_path,
        input_names=["pixel_values"],
        output_names=["logits"],
        opset_version=OPSET,
        do_constant_folding=True,
        dynamo=False,
    )
    checked = onnx.load(model_path)
    onnx.checker.check_model(checked)
    session = onnxruntime.InferenceSession(
        str(model_path),
        providers=["CPUExecutionProvider"],
    )
    runtime_logits = session.run(
        ["logits"],
        {"pixel_values": sample.cpu().numpy()},
    )[0]
    np.testing.assert_allclose(
        runtime_logits,
        reference_logits,
        rtol=0.0,
        atol=1e-4,
    )

    manifest = build_manifest(
        model_path,
        input_name="pixel_values",
        output_name="logits",
        labels=labels,
    )
    (output_directory / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    (output_directory / "reference-output.json").write_text(
        json.dumps(
            {"logits": reference_logits.tolist()},
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    print(
        f"status=exported version={manifest['asset']['version']} "
        f"bytes={manifest['file_bytes']}"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    export_model(arguments.output)


if __name__ == "__main__":
    main()
