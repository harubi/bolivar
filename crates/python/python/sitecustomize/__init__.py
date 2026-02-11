import contextlib
import sys


def _warn(exc: Exception) -> None:
    with contextlib.suppress(Exception):
        sys.stderr.write(f"bolivar autoload failed: {exc}\n")


try:
    from bolivar import _autoload as bolivar_autoload
except Exception as exc:
    _warn(exc)
else:
    bolivar_autoload.install()
