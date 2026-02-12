from bolivar._native_api import glyphname2unicode as _glyphname2unicode

glyphname2unicode: dict[str, str] = _glyphname2unicode()

__all__ = ["glyphname2unicode"]
