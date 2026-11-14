"""Axis tolerances win over the shared one, whatever the dict order."""

from typing import Any

import pytest

from bolivar._bolivar import _extract_tables_for_compat_page

GEOMETRY = {
    "page_bbox": (0, 0, 100, 100),
    "mediabox": (0, 0, 100, 100),
    "force_crop": True,
}
# Vertical lines 3 apart: a snap_x_tolerance of 1 keeps them apart (two
# columns); a tolerance of 5 snaps them together (one column). Two rows,
# because pdfplumber drops one-cell tables.
GRID = {
    "vertical_strategy": "explicit",
    "horizontal_strategy": "explicit",
    "explicit_vertical_lines": [10, 13, 50],
    "explicit_horizontal_lines": [10, 30, 50],
}


def columns(settings: dict[str, Any]) -> int:
    tables = _extract_tables_for_compat_page(
        [], [], [], [], GEOMETRY, table_settings=settings
    )
    return len(tables[0][0])


@pytest.mark.parametrize("axis_first", [True, False])
def test_axis_tolerance_wins_in_any_order(axis_first: bool) -> None:
    pairs = [("snap_x_tolerance", 1), ("snap_tolerance", 5)]
    settings = GRID | dict(pairs if axis_first else reversed(pairs))
    assert columns(settings) == 2


def test_shared_tolerance_fills_unset_axes() -> None:
    assert columns(GRID | {"snap_tolerance": 5}) == 1
    assert columns(GRID | {"snap_tolerance": 1}) == 2


@pytest.mark.parametrize(
    "settings",
    [
        {"snap_x_tolerance": -1, "snap_tolerance": 3},
        {"snap_tolerance": 3, "snap_x_tolerance": -1},
    ],
)
def test_bad_axis_tolerance_fails_in_any_order(settings: dict[str, Any]) -> None:
    with pytest.raises(ValueError, match="snap_x_tolerance"):
        columns(settings)


NICS = "crates/core/tests/fixtures/pdfplumber/nics-background-checks-2015-11.pdf"
TEXT = {"vertical_strategy": "text", "horizontal_strategy": "text"}


def text_tables(settings: dict[str, Any]) -> list[list[list[str | None]]]:
    import pdfplumber

    with pdfplumber.open(NICS) as pdf:
        return pdf.pages[0].extract_tables(TEXT | settings)


@pytest.mark.parametrize("axis_first", [True, False])
def test_text_axis_tolerance_wins_in_any_order(axis_first: bool) -> None:
    pairs = [("text_x_tolerance", 1), ("text_tolerance", 5)]
    settings = dict(pairs if axis_first else reversed(pairs))
    explicit = text_tables({"text_x_tolerance": 1, "text_y_tolerance": 5})
    # The page must care about the x tolerance, or this test proves nothing.
    assert explicit != text_tables({"text_x_tolerance": 5, "text_y_tolerance": 5})
    assert text_tables(settings) == explicit


def test_overridden_general_tolerance_is_still_checked() -> None:
    with pytest.raises(ValueError, match="snap_tolerance"):
        columns({"snap_tolerance": -1, "snap_x_tolerance": 1, "snap_y_tolerance": 1})


def test_infinite_tolerance_is_accepted_as_in_pdfplumber() -> None:
    columns(GRID | {"intersection_tolerance": float("inf")})


@pytest.mark.parametrize(
    ("settings", "named"),
    [
        ({"min_words_vertical": -1, "snap_tolerance": -1}, "snap_tolerance"),
        ({"vertical_strategy": "bogus", "snap_tolerance": -1}, "snap_tolerance"),
        ({"intersection_tolerance": -1, "min_words_horizontal": -1}, "min_words_horizontal"),
        (
            {"vertical_strategy": "bogus", "horizontal_strategy": "bogus"},
            "horizontal_strategy",
        ),
    ],
)
def test_first_error_is_pdfplumbers(settings: dict[str, Any], named: str) -> None:
    with pytest.raises(ValueError, match=named):
        columns(settings)
