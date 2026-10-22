"""Run selected Cargo tests with the controlled TLS environment fixtures require.

Only the child test process loses ambient TLS overrides. Native application
refusal and tests which explicitly inject these overrides remain unchanged.
"""
import os
import subprocess
import sys

TLS_OVERRIDES = ("SSL_CERT_FILE", "SSL_CERT_DIR", "SSLKEYLOGFILE")


def main(arguments: list[str]) -> int:
    environment = os.environ.copy()
    for name in TLS_OVERRIDES:
        environment.pop(name, None)
    return subprocess.run(["cargo", "test", *arguments], env=environment).returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
