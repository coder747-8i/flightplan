"""Read and write big GeoJSON FeatureCollections one feature at a time, so a 300 MB airspace
file never sits in memory whole (json.load of it took ~4 GB). Plain standard library.

    for f in features(path): ...           # any FeatureCollection file, one line or many
    with Writer(path) as w: w.write(f)     # one feature per line; still ordinary GeoJSON
"""
import json
from pathlib import Path

_DEC = json.JSONDecoder()
_CHUNK = 1 << 20


def features(path):
    """Yield the features of a FeatureCollection file in order. A file cut short (a download
    still in progress) yields the complete features it has."""
    with open(path, encoding="utf-8") as fh:
        buf = ""
        # find the start of the "features" array
        while True:
            i = buf.find('"features"')
            if i >= 0:
                j = buf.find("[", i)
                if j >= 0:
                    buf = buf[j + 1 :]
                    break
            more = fh.read(_CHUNK)
            if not more:
                return
            buf = buf[-16:] + more if i < 0 else buf + more
        eof = False
        pos = 0
        while True:
            n = len(buf)
            while pos < n and buf[pos] in " \t\r\n,":
                pos += 1
            if pos < n and buf[pos] == "]":
                return
            try:
                if pos >= n:
                    raise json.JSONDecodeError("need more", buf, pos)
                obj, end = _DEC.raw_decode(buf, pos)
            except json.JSONDecodeError:
                if eof:
                    return
                more = fh.read(_CHUNK)
                if not more:
                    eof = True
                # drop what's been read, keep the unfinished feature
                buf = buf[pos:] + more
                pos = 0
                continue
            yield obj
            pos = end


class Writer:
    """A FeatureCollection written as it goes, one feature per line."""

    def __init__(self, path):
        self.path = Path(path)
        self.count = 0

    def __enter__(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        self.fh = open(self.tmp, "w", encoding="utf-8")
        self.fh.write('{"type":"FeatureCollection","features":[\n')
        return self

    def write(self, feature):
        if self.count:
            self.fh.write(",\n")
        self.fh.write(json.dumps(feature, separators=(",", ":")))
        self.count += 1

    def __exit__(self, kind, *_):
        self.fh.write("\n]}\n")
        self.fh.close()
        if kind is None:
            self.tmp.replace(self.path)
        else:
            self.tmp.unlink(missing_ok=True)
        return False
