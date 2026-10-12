"""Verify the shipped module archive layout without compiling native code."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parent.parent


class PackageTests(unittest.TestCase):
    def test_verifier_extracts_module_into_private_home(self):
        """Nested Docker verification must own the extracted module directory."""
        target = ROOT / "target"; target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=target) as directory:
            root = Path(directory); home = root / "home"; home.mkdir()
            archive = root / "module.tar.gz"
            with tarfile.open(archive, "w:gz") as package:
                library = "libtinydocs_module.dylib" if sys.platform == "darwin" else "libtinydocs_module.so"
                for name, data in [(library, b"fixture"), ("modules.toml", b"fixture = true\n")]:
                    info = tarfile.TarInfo(name); info.size = len(data)
                    import io
                    package.addfile(info, io.BytesIO(data))
            bin_dir = root / "bin"; bin_dir.mkdir()
            cargo = bin_dir / "cargo"
            cargo.write_text(
                "#!/usr/bin/env python3\nimport os,sys\nfrom pathlib import Path\n"
                "library=Path(os.environ.get('TINYDOCS_TEST_MODULE') or sys.argv[sys.argv.index('--')+1]).resolve()\n"
                "assert library.is_file(), library\n"
                "assert library.is_relative_to(Path.home().resolve()), library\n"
            )
            cargo.chmod(0o755)
            env = dict(os.environ, HOME=str(home), PATH=str(bin_dir) + os.pathsep + os.environ["PATH"])
            env.pop("CARGO_TARGET_DIR", None)
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/verify-module.sh"), "--archive", str(archive)],
                cwd=ROOT, env=env, text=True, capture_output=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_arch_package_install_disables_only_landlock_filesystem_sandbox(self):
        """Pacman gets the filesystem-only sandbox exception in nested Arch builds."""
        target = ROOT / "target"; target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=target) as directory:
            root = Path(directory)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            bash = shutil.which("bash")
            self.assertIsNotNone(bash)
            (bin_dir / "bash").symlink_to(bash)
            capture = root / "pacman-args"
            pacman = bin_dir / "pacman"
            pacman.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$PACMAN_ARGS\"\n")
            pacman.chmod(0o755)
            env = {"PATH": str(bin_dir), "PACMAN_ARGS": str(capture)}
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/install-distro-packages.sh")],
                cwd=ROOT,
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            args = capture.read_text().splitlines()
            self.assertIn("--disable-sandbox-filesystem", args)
            self.assertNotIn("--disable-sandbox", args)
            self.assertEqual(
                args,
                ["-Syu", "--disable-sandbox-filesystem", "--noconfirm", "base-devel",
                 "cmake", "curl", "git", "gzip", "jq", "openssl", "perl"],
            )

    def test_archive_accepts_dotted_release_host_key_and_attests_library(self):
        """Package a dotted host key and preserve the archive attestation."""
        target = ROOT / "target"; target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=target) as directory:
            root = Path(directory); (root / "scripts").mkdir(); (root / "bin").mkdir()
            shutil.copy2(ROOT / "scripts/build-module.sh", root / "scripts/build-module.sh")
            for file in ["LICENSE", "README.md"]:
                (root / file).write_text("fixture")
            cargo = root / "bin/cargo"
            cargo.write_text(
                "#!/usr/bin/env python3\nimport sys\nfrom pathlib import Path\n"
                "target = sys.argv[sys.argv.index('--target') + 1]\n"
                "output = Path.cwd() / 'target' / target / 'release'\n"
                "output.mkdir(parents=True, exist_ok=True)\n"
                "(output / 'libtinydocs_module.so').write_bytes(b'compiled-fixture')\n"
            )
            cargo.chmod(0o755)
            env = dict(os.environ, PATH=str(root / "bin") + os.pathsep + os.environ["PATH"])
            env.pop("CARGO_TARGET_DIR", None)
            result = subprocess.run(
                ["bash", str(root / "scripts/build-module.sh"), "--crate", "tinydocs-module",
                 "--version", "0.2.2", "--target", "x86_64-unknown-linux-gnu",
                 "--host", "ubuntu-24.04-x86_64"],
                cwd=root, env=env, text=True, capture_output=True, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            archive = root / "release-assets/tinydocs-module-0.2.2-ubuntu-24.04-x86_64.tar.gz"
            with tarfile.open(archive) as package:
                data = package.extractfile("./libtinydocs_module.so").read()
                manifest = tomllib.loads(package.extractfile("./modules.toml").read().decode())
                self.assertEqual(data, b"compiled-fixture")
                self.assertEqual(manifest, {"libtinydocs_module.so": hashlib.sha256(data).hexdigest()})


if __name__ == "__main__":
    unittest.main()
