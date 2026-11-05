#!/bin/bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> 1. Creating namespace 'exavault'..."
kubectl apply -f "$DIR/namespace.yaml"

echo "==> 2. Copying ghcr-secret from default namespace if not exists..."
if ! kubectl get secret ghcr-secret -n exavault >/dev/null 2>&1; then
  if kubectl get secret ghcr-secret -n default >/dev/null 2>&1; then
    kubectl get secret ghcr-secret -n default -o json | jq 'del(.metadata.namespace, .metadata.resourceVersion, .metadata.uid, .metadata.creationTimestamp)' | kubectl apply -n exavault -f -
    echo "    ✅ ghcr-secret copied successfully."
  else
    echo "    ⚠️ Warning: ghcr-secret not found in default namespace. Please create it manually in exavault namespace."
  fi
else
  echo "    ℹ️ ghcr-secret already exists in exavault namespace."
fi

echo "==> 3. Applying bridges..."
kubectl apply -f "$DIR/bridges.yaml"

echo "==> 4. Checking secret..."
if [ -f "$DIR/secret.yaml" ]; then
  echo "    Applying secret.yaml..."
  kubectl apply -f "$DIR/secret.yaml"
else
  echo "    ℹ️ '$DIR/secret.yaml' not found. Please copy secret.example.yaml to secret.yaml, fill in your real values, and run:"
  echo "    kubectl apply -f $DIR/secret.yaml"
fi

echo "==> Done. Ready for Terraform deployment!"
