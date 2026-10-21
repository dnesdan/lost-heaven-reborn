"""No proprietary parser is implemented here. Source locations never enter manifests."""

import argparse
import fcntl
import hashlib
import http.client
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

REPO = Path(__file__).resolve().parents[1]
SOURCES = {"pc": "MAFIA_PC_URL", "ps2": "MAFIA_PS2_URL", "bios": "PS2_BIOS_URL"}


def _private_root(value=None):
    base = Path(os.environ.get("XDG_DATA_HOME", "/workspace/.local/share"))
    requested = Path(value or os.environ.get("LHR_PRIVATE_DIR", base / "lost-heaven-reborn/private")).expanduser()
    if requested.is_symlink():
        raise ValueError("private storage cannot be a symbolic link")
    root = requested.resolve()
    if root == REPO or REPO in root.parents:
        raise ValueError("private storage must be outside the repository")
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(root, 0o700)
    return root


def _write_json(path, value):
    fd, temporary = tempfile.mkstemp(prefix=".write-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(value, stream, indent=2, sort_keys=True, allow_nan=False)
            stream.write("\n")
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def _digest(path):
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


class _HTTPSRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        parsed = urllib.parse.urlsplit(newurl)
        if parsed.scheme != "https" or parsed.username or parsed.password:
            raise ValueError("unsafe download redirect refused")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def _download(source, downloads):
    """Resume only with a strong entity validator; keep URL and headers private."""
    parsed = urllib.parse.urlsplit(source)
    if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
        raise ValueError("download source must be HTTPS without embedded user information")
    key = hashlib.sha256(source.encode()).hexdigest()
    partial = downloads / (key + ".part")
    state_path = downloads / (key + ".json")
    state = {}
    if state_path.exists():
        state = json.loads(state_path.read_text(encoding="utf-8"))
    offset = partial.stat().st_size if partial.exists() else 0
    validator = state.get("etag", "")
    if not isinstance(validator, str) or not validator.startswith('"') or validator.startswith('W/'):
        offset = 0
    headers = {"Accept-Encoding": "identity", "User-Agent": "LostHeavenReborn-intake/1"}
    if offset:
        headers.update({"Range": f"bytes={offset}-", "If-Range": validator})
    request = urllib.request.Request(source, headers=headers)
    opener = urllib.request.build_opener(_HTTPSRedirect())
    try:
        response = opener.open(request, timeout=60)
    except urllib.error.HTTPError as exc:
        # A complete partial can produce 416. Keep it for diagnosis; never silently accept it.
        raise ValueError(f"download failed (HTTP {exc.code}); partial retained privately") from None
    except (urllib.error.URLError, OSError, ValueError):
        raise ValueError("download connection failed; source details suppressed") from None
    with response:
        status = response.status
        total = None
        if status == 206:
            match = re.fullmatch(r"bytes (\d+)-(\d+)/(\d+)", response.headers.get("Content-Range", ""))
            if not match or not offset or int(match[1]) != offset or int(match[2]) + 1 != int(match[3]):
                raise ValueError("server returned an invalid partial response")
            if response.headers.get("ETag") != validator:
                raise ValueError("server changed the download validator")
            total = int(match[3])
        elif status == 200:
            offset = 0
            length = response.headers.get("Content-Length")
            total = int(length) if length is not None else None
        else:
            raise ValueError("server returned an unsupported download response")
        etag = response.headers.get("ETag", "")
        _write_json(state_path, {"etag": etag if etag.startswith('"') else ""})
        with partial.open("ab" if offset else "wb") as stream:
            os.chmod(partial, 0o600)
            shutil.copyfileobj(response, stream, 1024 * 1024)
        if total is not None and partial.stat().st_size != total:
            raise ValueError("incomplete download; partial retained privately")
    return partial


def intake(kind, edition, local_path=None, private_root=None):
    """Copy a local file or download a configured source into immutable private storage."""
    if kind not in SOURCES:
        raise ValueError("kind must be pc, ps2, or bios")
    if not isinstance(edition, str) or not re.fullmatch(r"[\w .()+-]{1,100}", edition):
        raise ValueError("edition must be a short label without source locations")
    root = _private_root(private_root)
    for name in ("originals", "manifests", "downloads"):
        directory = root / name
        if directory.is_symlink():
            raise ValueError("private storage subdirectory cannot be a symbolic link")
        directory.mkdir(mode=0o700, exist_ok=True)
    with (root / ".intake.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if local_path is not None:
            source_path = Path(local_path).expanduser()
            if source_path.is_symlink() or not source_path.is_file():
                raise ValueError("local source must be a regular file, not a symbolic link")
            fd, temporary = tempfile.mkstemp(prefix="local-", dir=root / "downloads")
            with os.fdopen(fd, "wb") as output, source_path.open("rb") as source:
                shutil.copyfileobj(source, output, 1024 * 1024)
            partial = Path(temporary)
        else:
            source = os.environ.get(SOURCES[kind])
            if not source:
                raise ValueError(f"set {SOURCES[kind]} securely or pass --local")
            partial = _download(source, root / "downloads")
        digest = _digest(partial)
        original = root / "originals" / f"{digest}.bin"
        if original.exists():
            if original.is_symlink() or _digest(original) != digest:
                raise ValueError("stored original failed integrity verification")
            partial.unlink()
        else:
            os.replace(partial, original)
        os.chmod(original, 0o400)
        manifest = {"schema": "lhr-input-v1", "kind": kind, "edition": edition,
                    "sha256": digest, "size": original.stat().st_size,
                    "original": f"originals/{digest}.bin"}
        label = hashlib.sha256(edition.encode()).hexdigest()[:16]
        _write_json(root / "manifests" / f"{kind}-{label}-{digest}.json", manifest)
        return manifest


def _vector(value, positive=False):
    return (isinstance(value, list) and len(value) == 3 and
            all(type(n) in (int, float) and math.isfinite(n) and (not positive or n > 0) for n in value))


def validate_level(data):
    """Validate metadata only; a valid manifest does not prove GLB or game fidelity."""
    if not isinstance(data, dict) or set(data) - {"schema", "source", "units", "axes", "objects", "metadata"}:
        raise ValueError("invalid level fields")
    if data.get("schema") != "lhr-level-v1" or data.get("source") not in ("synthetic", "private"):
        raise ValueError("invalid schema or source")
    if data.get("units") != "meters" or data.get("axes") != "right-handed-y-up":
        raise ValueError("level must use meters and a right-handed Y-up coordinate system")
    if not isinstance(data.get("objects"), list):
        raise ValueError("objects must be an array")
    if "metadata" in data and not isinstance(data["metadata"], dict):
        raise ValueError("metadata must be an object")
    ids = set()
    for obj in data["objects"]:
        if not isinstance(obj, dict) or set(obj) - {"id", "model", "position", "rotation_degrees", "scale", "collision", "metadata"}:
            raise ValueError("invalid object fields")
        name = obj.get("id")
        if not isinstance(name, str) or not name.strip() or name in ids:
            raise ValueError("object ids must be nonempty and unique")
        ids.add(name)
        if not _vector(obj.get("position")):
            raise ValueError("position must contain three finite numbers")
        for key in ("rotation_degrees", "scale"):
            if key in obj and not _vector(obj[key], positive=key == "scale"):
                raise ValueError(f"invalid {key}")
        if "model" not in obj and "collision" not in obj:
            raise ValueError("object requires a model or collision")
        if "model" in obj:
            value = obj["model"]
            if (not isinstance(value, str) or "\\" in value or ":" in value or
                PurePosixPath(value).is_absolute() or ".." in value.split("/") or
                any(part in ("", ".") for part in value.split("/")) or not value.endswith(".glb")):
                raise ValueError("model must be a safe relative GLB path")
        if "collision" in obj:
            collision = obj["collision"]
            if (not isinstance(collision, dict) or set(collision) != {"shape", "size"} or
                collision["shape"] != "box" or not _vector(collision["size"], positive=True)):
                raise ValueError("invalid box collision")
        if "metadata" in obj and not isinstance(obj["metadata"], dict):
            raise ValueError("object metadata must be an object")
    # JSON serialization also rejects unsupported metadata values and nonfinite floats.
    try:
        json.dumps(data, allow_nan=False)
    except (ValueError, TypeError):
        raise ValueError("metadata must contain finite JSON values") from None
    return data


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    receive = commands.add_parser("intake", help="preserve private input; does not convert game formats")
    receive.add_argument("--kind", required=True, choices=SOURCES)
    receive.add_argument("--edition", required=True)
    receive.add_argument("--local")
    receive.add_argument("--private-dir")
    validate = commands.add_parser("validate-level")
    validate.add_argument("file", type=Path)
    validate.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "intake":
            result = intake(args.kind, args.edition, args.local, args.private_dir)
            print(f"Preserved {result['kind']}: SHA-256 {result['sha256']} ({result['size']} bytes).")
        else:
            result = validate_level(json.loads(args.file.read_text(encoding="utf-8")))
            if args.output:
                output = args.output.expanduser().resolve()
                if result["source"] == "private" and (output == REPO or REPO in output.parents):
                    raise ValueError("private level output must remain outside the repository")
                output.parent.mkdir(parents=True, exist_ok=True)
                _write_json(output, result)
            print(f"Validated {len(result['objects'])} intermediate objects; no proprietary conversion performed.")
    except (ValueError, OSError, urllib.error.URLError, http.client.HTTPException):
        # Never emit exception text: local paths, signed URLs and server headers can contain secrets.
        print("Intake/validation failed. Check input, schema, permissions and private source configuration.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
