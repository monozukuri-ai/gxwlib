"""Python argument handling; all format interpretation lives in Rust."""

import os

from . import _core


def load_csv(
    path: str | os.PathLike[str], *, device_profile: str, limits: _core.ReadLimits | None = None
) -> _core.InstructionProgram:
    """Read GX Works2 list CSV using an explicitly selected device syntax profile."""
    value = os.fspath(path)
    if not isinstance(value, str):
        raise TypeError("path must be str or os.PathLike[str]")
    return _core.load_csv(value, device_profile=device_profile, limits=limits)


def inspect(
    path: str | os.PathLike[str], *, limits: _core.ReadLimits | None = None
) -> _core.ProjectIndex:
    """Inspect CFB streams and logical metadata without decoding instructions."""
    value = os.fspath(path)
    if not isinstance(value, str):
        raise TypeError("path must be str or os.PathLike[str]")
    return _core.inspect(value, limits=limits)


def load_project(
    path: str | os.PathLike[str], *, limits: _core.ReadLimits | None = None
) -> _core.ParsedProject:
    """Load current POU candidates as raw records with source spans.

    Unknown or malformed POU payloads remain opaque with diagnostics.
    No instruction semantics or CPU settings are decoded.
    """
    value = os.fspath(path)
    if not isinstance(value, str):
        raise TypeError("path must be str or os.PathLike[str]")
    return _core.load_project(value, limits=limits)
