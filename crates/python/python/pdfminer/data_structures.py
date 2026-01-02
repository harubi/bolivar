# pdfminer.data_structures compatibility shim

from typing import Iterator, List, Optional, Tuple, Any

from .pdftypes import PDFObjRef, resolve1


class NumberTree:
    """Number tree for PDF structure.

    Used for page labels and structure tree.
    """

    def __init__(self, obj):
        self.obj = obj
        self._cache = None

    def _parse(self, obj, visited: Optional[set] = None) -> Iterator[Tuple[int, Any]]:
        """Parse the number tree recursively."""
        if visited is None:
            visited = set()
        if isinstance(obj, PDFObjRef):
            objid = getattr(obj, "objid", None)
            if objid is not None:
                if objid in visited:
                    return
                visited.add(objid)
        obj = resolve1(obj)
        if obj is None:
            return

        # Check for Nums array (leaf node)
        nums = obj.get("Nums") if isinstance(obj, dict) else None
        if nums:
            for i in range(0, len(nums), 2):
                key = resolve1(nums[i])
                value = nums[i + 1]
                yield (key, value)

        # Check for Kids array (intermediate node)
        kids = resolve1(obj.get("Kids")) if isinstance(obj, dict) else None
        if kids:
            for kid in kids:
                yield from self._parse(kid, visited)

    def __iter__(self) -> Iterator[Tuple[int, Any]]:
        """Iterate over (number, value) pairs."""
        if self._cache is None:
            self._cache = list(self._parse(self.obj))
        return iter(self._cache)

    @property
    def values(self) -> List[Tuple[int, Any]]:
        """Return cached (number, value) pairs."""
        if self._cache is None:
            self._cache = list(self._parse(self.obj))
        return self._cache

    def lookup(self, key: int) -> Optional[Any]:
        """Look up a value by its number key."""
        for k, v in self:
            if k == key:
                return v
            if k > key:
                break
        return None
