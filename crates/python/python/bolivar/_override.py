"""Internal ``@override`` decorator that needs no runtime dependencies.

``typing.override`` exists from Python 3.12. On 3.11, type checkers resolve
``typing_extensions.override`` from their bundled typeshed stubs, and the
runtime uses a local fallback with the same behaviour.
"""

import sys
from typing import TYPE_CHECKING

__all__ = ["override"]

if sys.version_info >= (3, 12):
    from typing import override
elif TYPE_CHECKING:
    from typing_extensions import override
else:
    from collections.abc import Callable
    from contextlib import suppress
    from typing import TypeVar

    _F = TypeVar("_F", bound=Callable[..., object])

    def override(method: _F, /) -> _F:
        """Mark *method* as an override, as ``typing.override`` does."""
        with suppress(AttributeError, TypeError):
            method.__override__ = True
        return method
