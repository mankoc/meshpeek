"""Lychee Slicer .lys scenes. Format notes: LYS_FILE_FORMAT.md at the repo root."""

from .codec import LysFormatError
from .loader import load

__all__ = ["LysFormatError", "load"]
