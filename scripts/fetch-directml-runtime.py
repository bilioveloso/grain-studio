"""Install the GPU (DirectML) ONNX Runtime for Windows x64 into src-tauri/resources.

The normal build downloads a CPU-only onnxruntime.dll. Run this once to use the graphics card for
AI features such as Real-ESRGAN upscaling instead (minutes become seconds). build.rs recognises the
DirectML runtime and keeps it.

Both packages come from nuget.org and are verified by SHA-256 before anything is extracted:
  Microsoft.ML.OnnxRuntime.DirectML 1.22.1  -> onnxruntime.dll, onnxruntime_providers_shared.dll
  Microsoft.AI.DirectML 1.15.4 (~200 MB)    -> DirectML.dll

Usage:
  python scripts/fetch-directml-runtime.py              # download both packages
  python scripts/fetch-directml-runtime.py --from DIR   # reuse .nupkg files already in DIR
"""

import argparse
import hashlib
import io
import sys
import urllib.request
import zipfile
from pathlib import Path

PACKAGES = [
    {
        "id": "microsoft.ml.onnxruntime.directml",
        "version": "1.22.1",
        "sha256": "210823f9932f81f222e2d05d04531d948a4d75dbf527dfb94195ad61371ab728",
        "files": {
            "runtimes/win-x64/native/onnxruntime.dll": (
                "onnxruntime.dll",
                "f722dd91a311b9fa4b7f2341d22c379a72fb7bc5b639df18b64821f23cdde14b",
            ),
            "runtimes/win-x64/native/onnxruntime_providers_shared.dll": (
                "onnxruntime_providers_shared.dll",
                "7de6659e55a0e02a8e7ca94ab2864d0e6ea59add87c3e947e8990229ae7f5e3a",
            ),
        },
    },
    {
        "id": "microsoft.ai.directml",
        "version": "1.15.4",
        "sha256": "4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9",
        "files": {
            "bin/x64-win/DirectML.dll": (
                "DirectML.dll",
                "9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1",
            ),
        },
    },
]

RESOURCES = Path(__file__).resolve().parent.parent / "src-tauri" / "resources"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def package_bytes(pkg: dict, local_dir: Path | None) -> bytes:
    name = f"{pkg['id']}.{pkg['version']}.nupkg"
    if local_dir:
        for candidate in local_dir.glob("*.nupkg"):
            data = candidate.read_bytes()
            if sha256(data) == pkg["sha256"]:
                print(f"using {candidate.name} for {name}")
                return data
    url = f"https://api.nuget.org/v3-flatcontainer/{pkg['id']}/{pkg['version']}/{name}"
    print(f"downloading {url}")
    return urllib.request.urlopen(url, timeout=600).read()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--from", dest="local_dir", type=Path, help="folder with already downloaded .nupkg files")
    args = parser.parse_args()

    RESOURCES.mkdir(parents=True, exist_ok=True)
    for pkg in PACKAGES:
        data = package_bytes(pkg, args.local_dir)
        if sha256(data) != pkg["sha256"]:
            print(f"refusing {pkg['id']} {pkg['version']}: package hash does not match", file=sys.stderr)
            return 1
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            for entry, (target, expected) in pkg["files"].items():
                content = archive.read(entry)
                if sha256(content) != expected:
                    print(f"refusing {entry}: file hash does not match", file=sys.stderr)
                    return 1
                (RESOURCES / target).write_bytes(content)
                print(f"installed resources/{target} ({len(content) / 1e6:.1f} MB)")
    print("DirectML runtime installed. Rebuild the app to use the GPU for AI features.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
