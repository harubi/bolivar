import itertools
from collections.abc import Callable, Hashable, Iterable
from operator import itemgetter
from typing import Any, TypeVar, cast

from .._typing import T_num, T_obj


def cluster_list(xs: list[T_num], tolerance: T_num = 0) -> list[list[T_num]]:
    if tolerance == 0:
        return [[x] for x in sorted(xs)]
    if len(xs) < 2:
        return [[x] for x in sorted(xs)]
    groups = []
    xs = sorted(xs)
    current_group = [xs[0]]
    last = xs[0]
    for x in xs[1:]:
        if x <= (last + tolerance):
            current_group.append(x)
        else:
            groups.append(current_group)
            current_group = [x]
        last = x
    groups.append(current_group)
    return groups


def make_cluster_dict(values: Iterable[T_num], tolerance: T_num) -> dict[T_num, int]:
    clusters = cluster_list(list(set(values)), tolerance)

    nested_tuples = [
        [(val, i) for val in value_cluster] for i, value_cluster in enumerate(clusters)
    ]

    return dict(itertools.chain(*nested_tuples))


Clusterable = TypeVar("Clusterable", T_obj, tuple[Any, ...])


def cluster_objects(
    xs: list[Clusterable],
    key_fn: Hashable | Callable[[Clusterable], T_num],
    tolerance: T_num,
    preserve_order: bool = False,
) -> list[list[Clusterable]]:
    resolved_key_fn: Callable[[Clusterable], T_num]
    if callable(key_fn):
        resolved_key_fn = cast("Callable[[Clusterable], T_num]", key_fn)
    else:
        resolved_key_fn = cast("Callable[[Clusterable], T_num]", itemgetter(key_fn))

    values = map(resolved_key_fn, xs)
    cluster_dict = make_cluster_dict(values, tolerance)

    get_0, get_1 = itemgetter(0), itemgetter(1)

    if preserve_order:
        cluster_tuples = [(x, cluster_dict[resolved_key_fn(x)]) for x in xs]
    else:
        cluster_tuples = sorted(
            ((x, cluster_dict[resolved_key_fn(x)]) for x in xs), key=get_1
        )

    grouped = itertools.groupby(cluster_tuples, key=get_1)

    return [list(map(get_0, values)) for _cluster_idx, values in grouped]
