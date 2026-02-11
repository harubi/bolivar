# pdfminer.casting compatibility shim (Rust-backed).

from bolivar._native_api import (
    safe_cmyk,
    safe_float,
    safe_int,
    safe_matrix,
    safe_rect,
    safe_rect_list,
    safe_rgb,
)

__all__ = [
    "safe_cmyk",
    "safe_float",
    "safe_int",
    "safe_matrix",
    "safe_rect",
    "safe_rect_list",
    "safe_rgb",
]
