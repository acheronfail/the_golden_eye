#!/usr/bin/env bash
set -euo pipefail

# Build plugin releases against the oldest OBS API we intentionally support.
#
# OBS 32 rejects plugins built against a newer major/minor libobs version, so
# building against 32.1 would prevent OBS 32.0.x users from loading the plugin.
# The OBS APIs used by this plugin, including ExtraBrowserDocks config support
# for the browser dock, are available in 31.0.0.
OBSAPI_VERSION="31.0.0"

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
git sparse-checkout set libobs UI/obs-frontend-api frontend/api
popd > /dev/null

rm -rf "${dest_dir}"
mkdir -p "${dest_dir}"
cp -r "${clone_dir}/obs-studio/libobs" "${dest_dir}/"
if [ -d "${clone_dir}/obs-studio/frontend/api" ]; then
  cp -r "${clone_dir}/obs-studio/frontend/api" "${dest_dir}/frontend"
else
  cp -r "${clone_dir}/obs-studio/UI/obs-frontend-api" "${dest_dir}/frontend"
fi

echo "${OBSAPI_VERSION}" > "${dest_dir}/OBS_VERSION"
