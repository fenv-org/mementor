#!/usr/bin/env bash
set -euo pipefail

# Download the BGE-small-en-v1.5 ONNX model from Hugging Face Hub.
# Skips download if the model file already exists.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODEL_DIR="${REPO_ROOT}/models/bge-small-en-v1.5"
MODEL_FILE="${MODEL_DIR}/model.onnx"

if [ -f "$MODEL_FILE" ] && [ "$(wc -c < "$MODEL_FILE")" -gt 1000000 ]; then
  echo "model.onnx already exists ($(du -h "$MODEL_FILE" | cut -f1)), skipping."
  exit 0
fi

echo "Downloading BGE-small-en-v1.5 model.onnx from Hugging Face..."
mkdir -p "$MODEL_DIR"
curl -fSL -o "$MODEL_FILE" \
  https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/model.onnx
echo "Downloaded $(du -h "$MODEL_FILE" | cut -f1) to $MODEL_FILE"
