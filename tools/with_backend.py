"""Run the existing fixture with this workspace's authoritative Cargo target."""

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--locked",
                "--offline",
                "--no-deps",
                "--format-version=1",
            ],
            cwd=ROOT,
            text=True,
        )
    )
    target = Path(metadata["target_directory"])
    if not target.is_absolute():
        raise ValueError("Cargo did not return an absolute target directory")
    return subprocess.call(
        [
            sys.executable,
            str(ROOT / "tests/fixtures/accounts/scripts/with-identity-server.py"),
            "just",
            "--justfile",
            str(ROOT / "justfile"),
            "backend-acceptance",
        ],
        cwd=ROOT,
        env=os.environ | {"CARGO_TARGET_DIR": str(target)},
    )


if __name__ == "__main__":
    raise SystemExit(main())
