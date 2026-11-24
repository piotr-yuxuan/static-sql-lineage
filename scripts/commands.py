# SPDX-License-Identifier: EUPL-1.2
"""What a command prints, for the scripts beside this one, which load it by its path."""

import subprocess


class CommandError(Exception):
    """A command that exited with a status other than 0."""


def output(command: tuple[str, ...], timeout: float, stdin: bytes = b"") -> bytes:
    """What `command` prints, given `stdin`; any failure stops the caller.

    CommandError, with what it printed on standard error, when it exits with a status other
    than 0; subprocess.TimeoutExpired when it runs past `timeout` seconds.
    """
    result = subprocess.run(command, input=stdin, capture_output=True, check=False, timeout=timeout)
    if result.returncode != 0:
        reason = result.stderr.decode(errors="replace").strip()
        message = f"{' '.join(command)} failed: {reason}"
        raise CommandError(message)
    return result.stdout
