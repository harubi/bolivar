import sys


def _warn(exc: Exception) -> None:
    try:
        sys.stderr.write(f"bolivar autoload failed: {exc}\n")
    except Exception:
        pass


try:
    import bolivar_autoload
except Exception as exc:
    _warn(exc)
else:
    bolivar_autoload.install()
