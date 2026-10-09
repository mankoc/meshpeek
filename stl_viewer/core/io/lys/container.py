"""Archive layer: V1 files are ZIP, V2+ files are MNGO (asar with `files` renamed to `mangoFiles`)."""

import json
import struct
import zipfile

from .codec import LysFormatError

ZIP_MAGIC = b"PK\x03\x04"


class Archive:
    def __init__(self, path):
        self._file = open(path, "rb")
        self.is_zip = self._file.read(4) == ZIP_MAGIC
        self._file.seek(0)
        if self.is_zip:
            self._zip = zipfile.ZipFile(self._file)
            self.names = set(self._zip.namelist())
        else:
            self._members = self._read_mngo_index()
            self.names = set(self._members)

    def _read_mngo_index(self) -> dict[str, tuple[int, int]]:
        head = self._file.read(16)
        if len(head) < 16:
            raise LysFormatError("file too short for an MNGO header")
        header_size, json_len = struct.unpack_from("<I", head, 4)[0], struct.unpack_from("<I", head, 12)[0]
        try:
            index = json.loads(self._file.read(json_len))
        except ValueError as e:
            raise LysFormatError(f"unreadable MNGO header: {e}") from e
        data_start = 8 + header_size
        members = {}

        def walk(node, prefix):
            for name, child in node["mangoFiles"].items():
                if "mangoFiles" in child:
                    walk(child, f"{prefix}{name}/")
                elif "offset" in child:  # symlinks and externally unpacked files are not used by .lys
                    members[prefix + name] = (data_start + int(child["offset"]), child["size"])

        walk(index, "")
        return members

    def __contains__(self, name: str) -> bool:
        return name in self.names

    def read(self, name: str) -> bytes:
        if self.is_zip:
            return self._zip.read(name)
        start, size = self._members[name]
        self._file.seek(start)
        return self._file.read(size)

    def close(self):
        self._file.close()

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
