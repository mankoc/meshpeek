"""Byte cipher + MessagePack used by scene.bin (V2+) and the islands members."""

import msgpack
import numpy as np

FILE_APP_ID = b"a8ee1146-8d03-4b69-8a67-59009a3f9ee7"
_KEY = np.frombuffer(FILE_APP_ID, np.uint8) % 255


class LysFormatError(ValueError):
    pass


def decipher(raw: bytes) -> bytes:
    data = np.frombuffer(raw, np.uint8)
    return (data - np.resize(_KEY, data.size)).tobytes()  # uint8 arithmetic wraps mod 256


def decode(raw: bytes, ciphered: bool = True):
    data = decipher(raw) if ciphered else raw
    try:
        return msgpack.unpackb(data, strict_map_key=False)
    except Exception as e:
        raise LysFormatError(f"scene data is not valid MessagePack ({e})") from e
