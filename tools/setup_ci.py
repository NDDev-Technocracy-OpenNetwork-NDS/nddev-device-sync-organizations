"""Prepare Linux credential-fixture prerequisites on disposable hosted workers."""

import os
import subprocess


def main():
    if (
        os.environ.get("GITHUB_ACTIONS") == "true"
        and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
        and os.environ.get("RUNNER_OS") == "Linux"
    ):
        subprocess.run(["sudo", "apt-get", "update", "-qq"], check=True)
        subprocess.run(
            [
                "sudo",
                "apt-get",
                "install",
                "-qq",
                "--no-install-recommends",
                "-y",
                "libsecret-1-dev",
                "gnome-keyring",
                "dbus-daemon",
                "libglib2.0-bin",
            ],
            check=True,
        )


if __name__ == "__main__":
    main()
