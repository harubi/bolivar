from collections.abc import Hashable, Sequence
from typing import TYPE_CHECKING, Any, Union

from .._typing import T_seq

if TYPE_CHECKING:  # pragma: nocover
    from pandas.core.frame import DataFrame


def to_list(collection: Union[T_seq, "DataFrame"]) -> list[Any]:
    if isinstance(collection, list):
        return collection
    elif isinstance(collection, Sequence):
        return list(collection)
    elif hasattr(collection, "to_dict"):
        res: list[dict[Hashable, Any]] = collection.to_dict(
            "records"
        )  # pragma: nocover
        return res
    else:
        return list(collection)
