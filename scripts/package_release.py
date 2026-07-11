#!/usr/bin/env python3
"""Create the platform archive consumed by the GitHub release workflow."""

from __future__ import annotations

import argparse
import os
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path


APP_NAME = "metronome-rs"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args()


def copy_notices(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    for name in ("LICENSE", "THIRD_PARTY_NOTICES"):
        shutil.copy2(name, destination / name)


def add_zip_tree(archive: zipfile.ZipFile, root: Path) -> None:
    for path in sorted(root.rglob("*")):
        if path.is_dir():
            continue
        archive_name = path.relative_to(root.parent).as_posix()
        info = zipfile.ZipInfo.from_file(path, archive_name)
        if os.access(path, os.X_OK):
            info.external_attr = (0o100755 & 0xFFFF) << 16
        with path.open("rb") as file:
            archive.writestr(info, file.read(), compress_type=zipfile.ZIP_DEFLATED)


def package_windows(binary: Path, staging: Path, output: Path, version: str, target: str) -> Path:
    root = staging / f"{APP_NAME}-v{version}-{target}"
    root.mkdir(parents=True)
    shutil.copy2(binary, root / f"{APP_NAME}.exe")
    copy_notices(root)
    archive_path = output / f"{root.name}.zip"
    with zipfile.ZipFile(archive_path, "w") as archive:
        add_zip_tree(archive, root)
    return archive_path


def package_linux(binary: Path, staging: Path, output: Path, version: str, target: str) -> Path:
    root = staging / f"{APP_NAME}-v{version}-{target}"
    root.mkdir(parents=True)
    installed_binary = root / APP_NAME
    shutil.copy2(binary, installed_binary)
    installed_binary.chmod(0o755)
    copy_notices(root)
    (root / f"{APP_NAME}.desktop").write_text(
        "\n".join(
            [
                "[Desktop Entry]",
                "Type=Application",
                "Name=metronome-rs",
                "Comment=Accurate desktop metronome",
                "Exec=metronome-rs",
                "Icon=metronome-rs",
                "Terminal=false",
                "Categories=Audio;Music;Utility;",
                "",
            ]
        ),
        encoding="utf-8",
    )
    shutil.copy2("assets/metronome-rs.svg", root / f"{APP_NAME}.svg")
    archive_path = output / f"{root.name}.tar.gz"
    with tarfile.open(archive_path, "w:gz") as archive:
        archive.add(root, arcname=root.name)
    return archive_path


def package_macos(binary: Path, staging: Path, output: Path, version: str, target: str) -> Path:
    bundle = staging / f"{APP_NAME}.app"
    macos = bundle / "Contents" / "MacOS"
    resources = bundle / "Contents" / "Resources"
    macos.mkdir(parents=True)
    resources.mkdir(parents=True)
    installed_binary = macos / APP_NAME
    shutil.copy2(binary, installed_binary)
    installed_binary.chmod(0o755)
    copy_notices(resources)
    create_macos_icon(Path("assets/metronome-rs.png"), resources / f"{APP_NAME}.icns", staging)
    with (bundle / "Contents" / "Info.plist").open("wb") as file:
        plistlib.dump(
            {
                "CFBundleDevelopmentRegion": "en",
                "CFBundleDisplayName": APP_NAME,
                "CFBundleExecutable": APP_NAME,
                "CFBundleIdentifier": "rs.metronome.desktop",
                "CFBundleInfoDictionaryVersion": "6.0",
                "CFBundleIconFile": f"{APP_NAME}.icns",
                "CFBundleName": APP_NAME,
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": version,
                "CFBundleVersion": version,
                "LSMinimumSystemVersion": "14.2",
                "NSHighResolutionCapable": True,
            },
            file,
        )
    archive_path = output / f"{APP_NAME}-v{version}-{target}.app.zip"
    with zipfile.ZipFile(archive_path, "w") as archive:
        add_zip_tree(archive, bundle)
    return archive_path


def create_macos_icon(source: Path, destination: Path, staging: Path) -> None:
    iconset = staging / f"{APP_NAME}.iconset"
    iconset.mkdir()
    sizes = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]
    for pixels, name in sizes:
        subprocess.run(
            ["sips", "-z", str(pixels), str(pixels), str(source), "--out", str(iconset / name)],
            check=True,
            stdout=subprocess.DEVNULL,
        )
    subprocess.run(
        ["iconutil", "-c", "icns", str(iconset), "-o", str(destination)],
        check=True,
    )


def main() -> None:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    executable = f"{APP_NAME}.exe" if "windows" in args.target else APP_NAME
    binary = Path("target") / args.target / "release" / executable
    if not binary.is_file():
        raise SystemExit(f"release binary not found: {binary}")

    with tempfile.TemporaryDirectory(prefix="metronome-release-") as temporary:
        staging = Path(temporary)
        if "windows" in args.target:
            archive = package_windows(binary, staging, args.output_dir, args.version, args.target)
        elif "apple-darwin" in args.target:
            archive = package_macos(binary, staging, args.output_dir, args.version, args.target)
        elif "linux" in args.target:
            archive = package_linux(binary, staging, args.output_dir, args.version, args.target)
        else:
            raise SystemExit(f"unsupported target: {args.target}")
    print(archive)


if __name__ == "__main__":
    main()
