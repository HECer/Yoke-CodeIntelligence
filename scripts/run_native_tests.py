"""Run selected Cargo tests with the controlled TLS environment fixtures require.

Only the child test process loses ambient TLS overrides. Native application
refusal and tests which explicitly inject these overrides remain unchanged.

--controlled-test-runner also cleans compiled host test executables after Cargo
has started. It refuses existing runner/target policy instead of replacing it.
Without the option, the original Cargo invocation is retained.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys

TLS_OVERRIDES = ("SSL_CERT_FILE", "SSL_CERT_DIR", "SSLKEYLOGFILE")


def controlled_environment():
    environment = os.environ.copy()
    for name in TLS_OVERRIDES:
        environment.pop(name, None)
    return environment


def check_runner_policy(environment):
    import tomllib

    if any(
        name.upper() == "CARGO_BUILD_TARGET"
        or name.upper().startswith("CARGO_TARGET_") and name.upper().endswith("_RUNNER")
        for name in environment
    ):
        raise ValueError("Existing target or runner environment")
    directories = [Path.cwd(), *Path.cwd().parents]
    cargo_home = Path(environment.get("CARGO_HOME", str(Path.home() / ".cargo")))
    candidates = [directory / ".cargo" for directory in directories] + [cargo_home]
    seen = set()
    for directory in candidates:
        # Cargo prefers the extensionless file when both forms exist.
        path = directory / "config"
        if not path.exists():
            path = directory / "config.toml"
        if path in seen or not path.exists():
            continue
        seen.add(path)
        if not path.is_file():
            raise ValueError("Unsupported Cargo configuration input")
        with path.open("rb") as source:
            data = source.read(65_537)
        if len(data) > 65_536:
            raise ValueError("Cargo configuration exceeds inspection budget")
        value = tomllib.loads(data.decode("utf8"))
        targets = value.get("target", {})
        build = value.get("build", {})
        configured_environment = value.get("env", {})
        if (
            "include" in value
            or not isinstance(targets, dict)
            or not isinstance(build, dict)
            or not isinstance(configured_environment, dict)
            or "target" in build
            or any(
                name.upper() == "CARGO_BUILD_TARGET"
                or name.upper().startswith("CARGO_TARGET_") and name.upper().endswith("_RUNNER")
                for name in configured_environment
            )
            or any(isinstance(target, dict) and "runner" in target for target in targets.values())
        ):
            raise ValueError("Existing or indirect target/runner policy")


def main(arguments: list[str]) -> int:
    environment = controlled_environment()
    if arguments[:1] == ["--test-executable"]:
        if len(arguments) < 2:
            return 2
        if any(name in os.environ for name in TLS_OVERRIDES):
            print("Controlled native test executable: removed ambient TLS overrides", file=sys.stderr)
        return subprocess.run(arguments[1:], env=environment).returncode
    if arguments[:1] == ["--controlled-test-runner"]:
        arguments = arguments[1:]
        cargo_options = arguments[:arguments.index("--")] if "--" in arguments else arguments
        if any(
            argument in ("--target", "--config")
            or argument.startswith(("--target=", "--config="))
            for argument in cargo_options
        ):
            return 2
        try:
            check_runner_policy(environment)
            version = subprocess.run(
                ["rustc", "-vV"], env=environment, capture_output=True, text=True
            )
            hosts = [line[6:].strip() for line in version.stdout.splitlines() if line.startswith("host: ")]
            if version.returncode or len(hosts) != 1 or not re.fullmatch(
                r"[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)+", hosts[0]
            ):
                return 2
        except (OSError, ValueError, ImportError):
            # Do not echo configuration contents, paths, or environment values.
            print("Controlled native test runner refuses unsupported host/runner configuration", file=sys.stderr)
            return 2
        runner = [sys.executable, str(Path(__file__).resolve()), "--test-executable"]
        configuration = f"target.{hosts[0]}.runner={json.dumps(runner, ensure_ascii=False)}"
        return subprocess.run(
            ["cargo", "--config", configuration, "test", *arguments], env=environment
        ).returncode
    return subprocess.run(["cargo", "test", *arguments], env=environment).returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
