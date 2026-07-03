#!/usr/bin/env bash
set -euo pipefail

OBSAPI_VERSION="32.1.2"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
dest_dir="${OBS_HEADERS_DIR:-${repo_root}/obs2/vendor/obs}"

if [ -d "${dest_dir}" ]; then
  echo "OBS Headers already found."
  echo "If you want to re-download, delete \"${dest_dir}\""
  exit 0
fi

clone_dir="$(mktemp -d)"
trap 'rm -rf "${clone_dir}"' EXIT

git clone \
  --depth 1 \
  --branch "${OBSAPI_VERSION}" \
  --filter=blob:none \
  --sparse \
  https://github.com/obsproject/obs-studio.git \
  "${clone_dir}/obs-studio"

pushd "${clone_dir}/obs-studio" > /dev/null
git sparse-checkout set libobs frontend/api
popd > /dev/null

rm -rf "${dest_dir}"
mkdir -p "${dest_dir}"
cp -r "${clone_dir}/obs-studio/libobs" "${dest_dir}/"
cp -r "${clone_dir}/obs-studio/frontend/api" "${dest_dir}/frontend"

echo "${OBSAPI_VERSION}" > "${dest_dir}/OBS_VERSION"
