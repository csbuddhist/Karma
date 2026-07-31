# PP-OCRv5 model notice

Karma's PP-OCRv5 export pipeline uses PaddleOCR, Copyright PaddlePaddle Authors, under the
Apache License 2.0 included in `LICENSE`.

- Official repository: <https://github.com/PaddlePaddle/PaddleOCR>
- Pinned release: `v3.5.0`
- Pinned commit: `33cbdd9deb2e00f61e7966db70669b249c005a37`
- Pinned model documentation:
  <https://github.com/PaddlePaddle/PaddleOCR/blob/33cbdd9deb2e00f61e7966db70669b249c005a37/docs/version3.x/pipeline_usage/OCR.en.md>
- License source:
  <https://github.com/PaddlePaddle/PaddleOCR/blob/33cbdd9deb2e00f61e7966db70669b249c005a37/LICENSE>

## Reviewed official downloads

| Model | Official inference archive | Bytes | SHA-256 |
|---|---|---:|---|
| `PP-OCRv5_mobile_det` | <https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_mobile_det_infer.tar> | 4,935,680 | `50446e5d01ac2a73d5319c89513281f6578414c888c602f9af13f93feefffc58` |
| `PP-OCRv5_mobile_rec` | <https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_mobile_rec_infer.tar> | 16,834,560 | `566b9512b34e34a9f0db54d87b51fa5a0b9ed2cf1ab7e49728cc0b8b5a64f414` |
| `PP-OCRv5_server_det` | <https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_server_det_infer.tar> | 88,340,480 | `22a33e0ba6a21425ea4192da03bf4395c9a0c67902bd924b7328fc859073045d` |
| `PP-OCRv5_server_rec` | <https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_server_rec_infer.tar> | 84,869,120 | `d99be2ffd348943ab52876179168be4fb5b14f5f0812f2ae4c76d89ec2ea750a` |

Paddle's official model documentation does not publish checksums for these archives. On
2026-07-27, the lengths and SHA-256 values above were established by downloading the exact listed
HTTPS responses and reviewing their expected archive names, roots, `Global.model_name` values, and
three-file inference layout. These hashes are local reproducibility and change-detection pins.
They must not be represented as publisher-provided, publisher-signed, or independently
authenticated checksums.

The conversion environment is locked to PaddlePaddle `3.0.0`, Paddle2ONNX `2.1.0`, ONNX `1.17.0`,
and ONNX Runtime `1.22.0`, plus fully pinned transitive dependencies in
`tools/ocr-export/requirements.lock`.
