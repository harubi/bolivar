from __future__ import annotations

from collections.abc import Iterable, Sequence
from typing import Any, Literal, Protocol, TypeAlias, TypedDict

T_seq: TypeAlias = Sequence
T_num: TypeAlias = int | float
T_point: TypeAlias = tuple[T_num, T_num]
T_bbox: TypeAlias = tuple[T_num, T_num, T_num, T_num]
T_obj: TypeAlias = dict[str, Any]
T_obj_list: TypeAlias = list[T_obj]
T_obj_iter: TypeAlias = Iterable[T_obj]
T_dir: TypeAlias = Literal["ltr"] | Literal["rtl"] | Literal["ttb"] | Literal["btt"]
T_kwargs: TypeAlias = dict[str, object]
T_any_kwargs: TypeAlias = dict[str, Any]
T_text_settings: TypeAlias = dict[str, object]
T_mapping_obj: TypeAlias = dict[str, object]


class StructElementDict(TypedDict, total=False):
    type: str
    revision: int
    id: str
    lang: str
    alt_text: str
    actual_text: str
    title: str
    page_number: int
    attributes: dict[str, object]
    mcids: list[int]
    children: list[StructElementDict]


T_struct_list: TypeAlias = list[StructElementDict]


class TableSettingsDict(TypedDict, total=False):
    vertical_strategy: str
    horizontal_strategy: str
    explicit_vertical_lines: list[T_obj | T_num]
    explicit_horizontal_lines: list[T_obj | T_num]
    snap_tolerance: T_num
    snap_x_tolerance: T_num
    snap_y_tolerance: T_num
    join_tolerance: T_num
    join_x_tolerance: T_num
    join_y_tolerance: T_num
    edge_min_length: T_num
    edge_min_length_prefilter: T_num
    min_words_vertical: int
    min_words_horizontal: int
    intersection_tolerance: T_num
    intersection_x_tolerance: T_num
    intersection_y_tolerance: T_num
    text_settings: T_text_settings


TableSettingsInput: TypeAlias = TableSettingsDict | dict[str, object]


class MarkedContentObject(Protocol):
    mcid: int | None
    tag: str | None


class ContainerWithMarkedContent(Protocol):
    _objs: list[MarkedContentObject]


class PageAggregatorRenderHooks(Protocol):
    def render_char(self, *args: object, **kwargs: object) -> float: ...

    def render_image(self, *args: object, **kwargs: object) -> None: ...

    def paint_path(self, *args: object, **kwargs: object) -> None: ...
