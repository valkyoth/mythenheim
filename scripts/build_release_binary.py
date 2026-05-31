#!/usr/bin/env python3
"""Build a clean native Mythenheim release binary and print SHA256 values."""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path


DEFAULT_REPO = "https://github.com/valkyoth/mythenheim.git"
PACKAGE_NAME = "mythenheim"
SUPPORTED_PLATFORMS = ("linux", "macos", "windows")


def main() -> int:
    args = parse_args()
    requested_platform = args.platform.lower()
    os_label = normalize_label(args.os_label or requested_platform, "OS label")
    host_platform = detect_host_platform()
    if requested_platform != host_platform and not args.allow_platform_mismatch:
        fail(
            f"requested {requested_platform}, but this host looks like {host_platform}. "
            "Run this script on the target OS, or pass --allow-platform-mismatch "
            "only when you know the build environment is correct."
        )

    env = os.environ.copy()
    ensure_prerequisites(args.install_prereqs, env)

    work_root = args.work_dir.resolve()
    out_dir = args.out_dir.resolve()
    work_root.mkdir(parents=True, exist_ok=True)
    out_dir.mkdir(parents=True, exist_ok=True)

    clone_dir = work_root / f"{PACKAGE_NAME}-{requested_platform}-{machine_label()}"
    if clone_dir.exists():
        shutil.rmtree(clone_dir)

    run(["git", "clone", args.repo, str(clone_dir)])
    run(["git", "checkout", args.ref], cwd=clone_dir)

    commit = output(["git", "rev-parse", "HEAD"], cwd=clone_dir).strip()
    version = cargo_version(clone_dir / "Cargo.toml")
    tag = validate_release_tag(clone_dir, version, args.allow_untagged)
    ensure_rust_toolchain(clone_dir, args.install_prereqs, env)
    if args.target and args.install_prereqs and shutil.which("rustup") is not None:
        run(["rustup", "target", "add", args.target], env=env)

    build_command = ["cargo", "build", "--release", "--locked"]
    if args.target:
        build_command.extend(["--target", args.target])
    run(build_command, cwd=clone_dir, env=env)

    package = package_release(clone_dir, out_dir, requested_platform, os_label, args.target)
    binary = binary_path(clone_dir, requested_platform, args.target)
    package_sha = sha256_file(package)
    binary_sha = sha256_file(binary)

    print()
    print("release binary build: ok")
    print(f"repository: {args.repo}")
    print(f"commit: {commit}")
    print(f"tag: {tag or 'untagged'}")
    print(f"platform: {requested_platform}")
    print(f"os label: {os_label}")
    print(f"target: {args.target or native_target_label()}")
    print(f"artifact: {package}")
    print(f"artifact sha256: {package_sha}")
    print(f"binary sha256: {binary_sha}")
    print()
    print("Release-note lines:")
    print(f"- {package.name}: `{package_sha}`")
    print(f"- {binary.name}: `{binary_sha}`")

    if not args.keep_work:
        shutil.rmtree(clone_dir)

    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Clone Mythenheim, build the native release binary, package it for a "
            "GitHub release, and print SHA256 values."
        )
    )
    parser.add_argument("platform", choices=SUPPORTED_PLATFORMS)
    parser.add_argument("--repo", default=DEFAULT_REPO, help=f"Git repository to clone. Default: {DEFAULT_REPO}")
    parser.add_argument(
        "--ref",
        required=True,
        help="Git tag to check out before building. Release builds must use v<package version>.",
    )
    parser.add_argument(
        "--os-label",
        help=(
            "Artifact OS label. Defaults to the platform argument. Useful for "
            "variants such as windows11 or windowsserver2026."
        ),
    )
    parser.add_argument(
        "--target",
        help=(
            "Optional Rust target triple, for example aarch64-unknown-linux-gnu "
            "or aarch64-apple-darwin. Native host builds do not need this."
        ),
    )
    parser.add_argument(
        "--work-dir",
        type=Path,
        default=Path("target/release-build-work"),
        help="Temporary clone directory root.",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=Path("target/release-binaries"),
        help="Directory for packaged release artifacts.",
    )
    parser.add_argument(
        "--install-prereqs",
        action="store_true",
        help="Install rustup/Rust when missing. Git and Python must already be available.",
    )
    parser.add_argument(
        "--allow-platform-mismatch",
        action="store_true",
        help="Allow the platform argument to differ from the detected host OS.",
    )
    parser.add_argument(
        "--allow-untagged",
        action="store_true",
        help="Allow building a non-tagged ref. For local validation only, not release artifacts.",
    )
    parser.add_argument(
        "--keep-work",
        action="store_true",
        help="Keep the cloned build tree after a successful build.",
    )
    return parser.parse_args()


def detect_host_platform() -> str:
    system = platform.system().lower()
    if system == "linux":
        return "linux"
    if system == "darwin":
        return "macos"
    if system == "windows":
        return "windows"
    if system in {"freebsd", "openbsd", "netbsd", "dragonfly"}:
        return "bsd"
    fail(f"unsupported host OS: {platform.system()}")


def machine_label() -> str:
    machine = platform.machine().lower() or "unknown"
    return normalize_label(machine, "machine label")


def native_target_label() -> str:
    return f"native-{machine_label()}"


def normalize_label(value: str, description: str) -> str:
    label = re.sub(r"[^a-z0-9_]+", "-", value.lower()).strip("-")
    if not label:
        fail(f"{description} cannot be empty")
    return label


def target_arch_label(target: str | None) -> str:
    if not target:
        return machine_label()
    return normalize_label(target.split("-", maxsplit=1)[0], "target architecture")


def ensure_prerequisites(install_prereqs: bool, env: dict[str, str]) -> None:
    if shutil.which("git") is None:
        fail("git is required before this script can clone the repository.")

    if shutil.which("cargo") is None:
        if not install_prereqs:
            fail("cargo is missing. Install Rust, or rerun with --install-prereqs.")
        install_rustup(env)

    cargo_home_bin = cargo_home() / "bin"
    env["PATH"] = f"{cargo_home_bin}{os.pathsep}{env.get('PATH', '')}"


def ensure_rust_toolchain(clone_dir: Path, install_prereqs: bool, env: dict[str, str]) -> None:
    channel = rust_channel(clone_dir / "rust-toolchain.toml")
    if shutil.which("rustup") is None:
        if not install_prereqs:
            print("warning: rustup not found; using existing cargo/rustc from PATH", file=sys.stderr)
            return
        install_rustup(env)

    if command_succeeds(["rustup", "run", channel, "rustc", "--version"], env=env):
        return

    if not install_prereqs:
        fail(f"Rust toolchain {channel} is missing. Rerun with --install-prereqs.")

    run(["rustup", "toolchain", "install", channel, "--profile", "minimal"], env=env)


def install_rustup(env: dict[str, str]) -> None:
    if platform.system().lower() == "windows":
        url = "https://win.rustup.rs/x86_64"
        with tempfile.TemporaryDirectory() as tmp:
            installer = Path(tmp) / "rustup-init.exe"
            urllib.request.urlretrieve(url, installer)
            run([str(installer), "-y", "--profile", "minimal"], env=env)
    else:
        url = "https://sh.rustup.rs"
        with tempfile.TemporaryDirectory() as tmp:
            installer = Path(tmp) / "rustup-init.sh"
            urllib.request.urlretrieve(url, installer)
            run(["sh", str(installer), "-y", "--profile", "minimal"], env=env)

    cargo_home_bin = cargo_home() / "bin"
    env["PATH"] = f"{cargo_home_bin}{os.pathsep}{env.get('PATH', '')}"


def cargo_home() -> Path:
    value = os.environ.get("CARGO_HOME")
    if value:
        return Path(value).expanduser()
    if platform.system().lower() == "windows":
        user_profile = os.environ.get("USERPROFILE")
        if not user_profile:
            fail("USERPROFILE is not set; cannot locate Cargo home.")
        return Path(user_profile) / ".cargo"
    return Path.home() / ".cargo"


def rust_channel(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    match = re.search(r'^channel\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not match:
        fail(f"could not read Rust channel from {path}")
    return match.group(1)


def validate_release_tag(clone_dir: Path, version: str, allow_untagged: bool) -> str | None:
    result = subprocess.run(
        ["git", "describe", "--tags", "--exact-match", "HEAD"],
        cwd=clone_dir,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        if allow_untagged:
            return None
        fail(
            "release binary builds must check out an exact tag; "
            "use --allow-untagged only for local validation"
        )

    tag = result.stdout.strip()
    expected = f"v{version}"
    if tag != expected:
        fail(
            f"checked-out tag {tag} does not match package version {version}; "
            f"expected {expected}"
        )
    return tag


def package_release(
    clone_dir: Path,
    out_dir: Path,
    requested_platform: str,
    os_label: str,
    target: str | None,
) -> Path:
    version = cargo_version(clone_dir / "Cargo.toml")
    arch = target_arch_label(target)
    stem = f"{PACKAGE_NAME}-{version}-{os_label}-{arch}"
    binary = binary_path(clone_dir, requested_platform, target)

    if requested_platform == "windows":
        package = out_dir / f"{stem}.zip"
        with zipfile.ZipFile(package, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.write(binary, arcname=binary.name)
            add_if_exists(archive, clone_dir / "LICENSE", "LICENSE")
            add_if_exists(archive, clone_dir / "NOTICE", "NOTICE")
            add_if_exists(archive, clone_dir / "README.md", "README.md")
        return package

    package = out_dir / f"{stem}.tar.gz"
    with tarfile.open(package, "w:gz") as archive:
        archive.add(binary, arcname=f"{stem}/{binary.name}")
        add_tar_if_exists(archive, clone_dir / "LICENSE", f"{stem}/LICENSE")
        add_tar_if_exists(archive, clone_dir / "NOTICE", f"{stem}/NOTICE")
        add_tar_if_exists(archive, clone_dir / "README.md", f"{stem}/README.md")
    return package


def binary_path(clone_dir: Path, requested_platform: str, target: str | None) -> Path:
    is_windows = requested_platform == "windows" or bool(target and "windows" in target)
    name = f"{PACKAGE_NAME}.exe" if is_windows else PACKAGE_NAME
    if target:
        path = clone_dir / "target" / target / "release" / name
    else:
        path = clone_dir / "target" / "release" / name
    if not path.exists():
        fail(f"release binary was not created: {path}")
    return path


def cargo_version(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not match:
        fail(f"could not read package version from {path}")
    return match.group(1)


def add_if_exists(archive: zipfile.ZipFile, path: Path, arcname: str) -> None:
    if path.exists():
        archive.write(path, arcname=arcname)


def add_tar_if_exists(archive: tarfile.TarFile, path: Path, arcname: str) -> None:
    if path.exists():
        archive.add(path, arcname=arcname)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(
    command: list[str],
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
) -> None:
    print(f"+ {' '.join(command)}", flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True)


def command_succeeds(command: list[str], env: dict[str, str] | None = None) -> bool:
    result = subprocess.run(command, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return result.returncode == 0


def output(command: list[str], cwd: Path | None = None) -> str:
    return subprocess.check_output(command, cwd=cwd, text=True)


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


if __name__ == "__main__":
    raise SystemExit(main())
