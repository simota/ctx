"""Python fixture: nested defs, methods, decorators, async, dedup."""
from typing import Optional


def top_level(x: int) -> int:
    def inner(y: int) -> int:
        return x + y

    return inner(x)


class Service:
    def __init__(self, name: str) -> None:
        self.name = name

    @property
    def label(self) -> str:
        return self.name

    async def fetch(self) -> Optional[str]:
        return self.name

    class Inner:
        def helper(self) -> None:
            pass


def top_level(x):  # duplicate name+kind → deduped (first kept)
    return x


async def standalone() -> None:
    pass
