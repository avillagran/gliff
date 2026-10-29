"""Install gliff for the current user, run on the remote machine.

gliff sends this script over ssh to `python3 -I -` and reads one JSON line
back. It downloads the gliff package from the Omarchy repository configured in
/etc/pacman.conf, checks it against the repository database and the machine's
own pacman keyring, and copies the binaries to ~/.local. Nothing runs as root
and nothing outside the user's home changes; the download is deleted.
"""

import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.request

PACMAN_CONF = pathlib.Path("/etc/pacman.conf")
KEYRING = pathlib.Path("/etc/pacman.d/gnupg")
HOME = pathlib.Path.home()
MANIFEST = HOME / ".local/share/gliff/user-install.json"
# Package path -> where it goes, relative to the home directory.
FILES = {
    "usr/bin/gliff": ".local/bin/gliff",
    "usr/bin/gliff-server": ".local/bin/gliff-server",
    "usr/share/applications/gliff.desktop": ".local/share/applications/gliff.desktop",
    "usr/share/icons/hicolor/scalable/apps/gliff.svg": ".local/share/icons/hicolor/scalable/apps/gliff.svg",
}
BINARIES = ["usr/bin/gliff", "usr/bin/gliff-server"]


class Refused(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def reply(**fields):
    print(json.dumps(fields), flush=True)


def omarchy_server():
    """The first Server of the [omarchy] section, with $repo and $arch filled in."""
    section = None
    for raw in PACMAN_CONF.read_text().splitlines():
        line = raw.split("#", 1)[0].strip()
        m = re.fullmatch(r"\[(.+)\]", line)
        if m:
            section = m[1]
            continue
        m = re.fullmatch(r"Server\s*=\s*(\S+)", line)
        if section == "omarchy" and m:
            url = m[1].replace("$repo", "omarchy").replace("$arch", platform.machine())
            if not url.startswith("https://"):
                raise Refused("repository", "The Omarchy repository does not use https")
            return url.rstrip("/")
    raise Refused("repository", "No Omarchy package repository is configured in /etc/pacman.conf")


def fetch(url, limit):
    try:
        with urllib.request.urlopen(url, timeout=30) as r:
            data = r.read(limit + 1)
    except OSError as e:
        raise Refused("download", f"Download failed: {e}") from None
    if len(data) > limit:
        raise Refused("download", "Download larger than expected")
    return data


def run(argv, **kw):
    return subprocess.run(argv, capture_output=True, text=True, timeout=60, **kw)


def package_entry(db):
    """FILENAME, VERSION and SHA256SUM of gliff from a repository database."""
    names = run(["bsdtar", "-tf", str(db)])
    if names.returncode:
        raise Refused("repository", "Unreadable repository database")
    entries = [n for n in names.stdout.splitlines() if re.fullmatch(r"gliff-[^-/]+-[^-/]+/desc", n)]
    if len(entries) != 1:
        raise Refused("repository", "gliff is not in the Omarchy repository")
    desc = run(["bsdtar", "-xOf", str(db), entries[0]]).stdout
    fields, key = {}, None
    for line in desc.splitlines():
        if re.fullmatch(r"%[A-Z0-9]+%", line):
            key = line.strip("%")
        elif key and line and key not in fields:
            fields[key] = line
    if fields.get("NAME") != "gliff":
        raise Refused("repository", "Unexpected repository entry")
    name = fields.get("FILENAME", "")
    if not re.fullmatch(r"gliff-[0-9A-Za-z.+_:~-]+\.pkg\.tar\.(zst|xz|gz)", name):
        raise Refused("repository", "Unexpected package file name")
    if not re.fullmatch(r"[0-9a-f]{64}", fields.get("SHA256SUM", "")):
        raise Refused("repository", "The repository lists no checksum for gliff")
    return name, fields.get("VERSION", "?"), fields["SHA256SUM"]


def verify_signature(work, package, signature):
    """Accept only a signature that the machine's own pacman keyring trusts."""
    home = work / "gnupg"
    home.mkdir(mode=0o700)
    copied = False
    for name in ["pubring.gpg", "pubring.kbx", "trustdb.gpg"]:
        if (KEYRING / name).is_file():
            shutil.copyfile(KEYRING / name, home / name)
            copied = True
    if not copied:
        raise Refused("signature", "The pacman keyring is not readable")
    r = run(["gpg", "--homedir", str(home), "--batch", "--no-auto-key-retrieve",
             "--status-fd", "1", "--verify", str(signature), str(package)])
    status = r.stdout.split("\n")
    valid = any(line.startswith("[GNUPG:] VALIDSIG ") for line in status)
    trusted = any(line.startswith(("[GNUPG:] TRUST_FULLY", "[GNUPG:] TRUST_ULTIMATE")) for line in status)
    if r.returncode or not valid or not trusted:
        raise Refused("signature", "The package signature is not trusted by this machine's pacman keyring")


def missing_libraries(paths):
    missing = set()
    for path in paths:
        r = run(["ldd", str(path)])
        missing.update(line.split()[0] for line in r.stdout.splitlines() if "not found" in line)
    return sorted(missing)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def install(staged, version):
    """Copy staged files into the home directory, replacing only our own."""
    try:
        owned = json.loads(MANIFEST.read_text()).get("files", {})
    except FileNotFoundError:
        owned = {}
    for dest in FILES.values():
        target = HOME / dest
        if not os.path.lexists(target):
            continue
        meta = target.lstat()
        ours = (stat.S_ISREG(meta.st_mode) and meta.st_uid == os.getuid()
                and owned.get(dest) == digest(target))
        if not ours:
            raise Refused("conflict", f"~/{dest} already exists and was not installed by gliff")
    installed = {}
    for source, dest in FILES.items():
        target = HOME / dest
        target.parent.mkdir(parents=True, exist_ok=True)
        data = (staged / source).read_bytes()
        if source.endswith(".desktop"):
            # Launchers do not always see ~/.local/bin on PATH.
            text = re.sub(r"^Exec=gliff\b", f"Exec={HOME / '.local/bin/gliff'}", data.decode(), flags=re.M)
            data = text.encode()
        fd, tmp = tempfile.mkstemp(prefix=".gliff-", dir=target.parent)
        try:
            with os.fdopen(fd, "wb") as out:
                out.write(data)
            os.chmod(tmp, 0o755 if source in BINARIES else 0o644)
            os.replace(tmp, target)
        finally:
            if os.path.exists(tmp):
                os.unlink(tmp)
        installed[dest] = hashlib.sha256(data).hexdigest()
    MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text(json.dumps({"version": version, "files": installed}, indent=2) + "\n")


def main():
    found = shutil.which("gliff-server")
    if found:
        return reply(ok=True, status="present", path=found)
    if not (shutil.which("Hyprland") or shutil.which("hyprctl")):
        raise Refused("hyprland", "gliff needs Hyprland on that machine")
    for tool in ["bsdtar", "gpg", "ldd"]:
        if not shutil.which(tool):
            raise Refused("dependency", f"{tool} is missing on that machine")
    base = omarchy_server()
    with tempfile.TemporaryDirectory(prefix="gliff-install-") as d:
        work = pathlib.Path(d)
        db = work / "omarchy.db"
        db.write_bytes(fetch(f"{base}/omarchy.db", 64 << 20))
        name, version, sha256 = package_entry(db)
        package, signature = work / name, work / (name + ".sig")
        package.write_bytes(fetch(f"{base}/{name}", 256 << 20))
        if digest(package) != sha256:
            raise Refused("checksum", "The package does not match the repository checksum")
        signature.write_bytes(fetch(f"{base}/{name}.sig", 64 << 10))
        verify_signature(work, package, signature)
        staged = work / "root"
        staged.mkdir()
        r = run(["bsdtar", "-xf", str(package), "-C", str(staged), *FILES])
        if r.returncode or not all((staged / f).is_file() for f in FILES):
            raise Refused("package", "The package does not contain the expected files")
        missing = missing_libraries(staged / b for b in BINARIES)
        if missing:
            raise Refused("libraries", "Missing libraries on that machine: " + ", ".join(missing))
        install(staged, version)
    bin_dir = str(HOME / ".local/bin")
    reply(ok=True, status="installed", version=version, path=str(HOME / FILES["usr/bin/gliff-server"]),
          on_path=bin_dir in os.environ.get("PATH", "").split(":"))


if __name__ == "__main__":
    try:
        main()
    except Refused as e:
        reply(ok=False, code=e.code, message=str(e))
    except Exception as e:  # Report, never a traceback the client cannot read.
        reply(ok=False, code="error", message=f"{type(e).__name__}: {e}"[:300])
