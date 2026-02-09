import sys


def _warn(exc: Exception) -> None:
    try:
        sys.stderr.write(f"bolivar autoload failed: {exc}\n")
    except Exception:
        pass


def _install_with_top_level_autoload() -> bool:
    import bolivar_autoload

    return bool(bolivar_autoload.install())


def _install_with_shim_registry() -> bool:
    import bolivar._shim_registry as shim_registry

    shim_registry.install()
    return True


def install() -> bool:
    try:
        return _install_with_top_level_autoload()
    except Exception as primary_exc:
        try:
            return _install_with_shim_registry()
        except Exception as fallback_exc:
            _warn(primary_exc)
            _warn(fallback_exc)
            return False
