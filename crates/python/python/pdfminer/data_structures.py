# pdfminer.data_structures compatibility shim

from typing import Iterator, List, Optional, Tuple, Any

from .pdftypes import resolve1


class NumberTree:
    """Number tree for PDF structure.

    Used for page labels and structure tree.
    """

    def __init__(self, obj):
        self.obj = obj
        self._cache = None

    def _parse(self, obj) -> Iterator[Tuple[int, Any]]:
        """Parse the number tree recursively."""
        obj = resolve1(obj)
        if obj is None:
            return

        # Check for Nums array (leaf node)
        nums = resolve1(obj.get("Nums")) if isinstance(obj, dict) else None
        if nums:
            for i in range(0, len(nums), 2):
                key = resolve1(nums[i])
                value = resolve1(nums[i + 1])
                yield (key, value)

        # Check for Kids array (intermediate node)
        kids = resolve1(obj.get("Kids")) if isinstance(obj, dict) else None
        if kids:
            for kid in kids:
                yield from self._parse(kid)

    def __iter__(self) -> Iterator[Tuple[int, Any]]:
        """Iterate over (number, value) pairs."""
        if self._cache is None:
            self._cache = list(self._parse(self.obj))
        return iter(self._cache)

    def lookup(self, key: int) -> Optional[Any]:
        """Look up a value by its number key."""
        for k, v in self:
            if k == key:
                return v
            if k > key:
                break
        return None
