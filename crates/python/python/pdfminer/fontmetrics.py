from typing import Any

from bolivar._native_api import font_metrics as _font_metrics

FONT_METRICS: dict[str, tuple[dict[str, Any], dict[str, float]]] = _font_metrics()

__all__ = ["FONT_METRICS"]
