# SPDX-License-Identifier: EUPL-1.2
"""What a command prints, for the scripts beside this one, which load it by its path."""

import subprocess


class ScriptError(Exception):
    """What stops a script beside this one, said in one line."""


def output(command: tuple[str, ...], timeout: float, stdin: bytes = b"") -> bytes:
    """What `command` prints, given `stdin`; its standard error as a ScriptError if it fails."""
    result = subprocess.run(command, input=stdin, capture_output=True, check=False, timeout=timeout)
    if result.returncode != 0:
        reason = " ".join(result.stderr.decode(errors="replace").split())
        message = f"{' '.join(command)} failed: {reason}"
        raise ScriptError(message)
    return result.stdout
