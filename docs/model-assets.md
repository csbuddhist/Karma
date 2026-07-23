# Model assets

Karma does not download image models at runtime and does not commit production model weights to
Git. The Windows Agent accepts a local, verified manifest generated from the pinned upstream model.

## Export

Use Python 3.11 or 3.12 in an isolated environment:

```bash
python3 -m venv tools/model-export/.venv
tools/model-export/.venv/bin/pip install -r tools/model-export/requirements.txt
tools/model-export/.venv/bin/python tools/model-export/export_viddexa.py \
  --output target/model-assets/viddexa-nano
```

The exporter:

- loads only safetensors from `viddexa/nsfw-detection-2-nano`;
- fixes revision `913bc502e69fa3edfe2cfce72c98cad4ddc6149b`;
- disables remote model code;
- verifies upstream labels and preprocessing parameters;
- replaces the upstream oversized average-pool operation with the equivalent adaptive global
  average operation after confirming their logits match;
- exports static float32 NCHW input `[1,3,224,224]` with opset 18;
- checks the ONNX graph and compares it with PyTorch on generated, non-sensitive input;
- writes the file length and SHA-256 into `manifest.json`.

The upstream index-zero label is `safe`; the product manifest normalizes this to `normal`. Other
labels retain their upstream names and indices.

## Packaging

The package pipeline copies `model.onnx`, `manifest.json`, and the Apache-2.0 model license into the
read-only installation asset directory. At runtime, Karma verifies the manifest, file length,
SHA-256, graph input/output names, element types, and static shapes before the first inference.
