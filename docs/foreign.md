# RDP and RustDesk machines

gliff shows and drives machines that run a Remote Desktop (RDP) server or
RustDesk instead of gliff. Their names carry the protocol:

```
rdp://user@host         # port 3389
rdp://user@host:3390
rustdesk://host         # port 21118, RustDesk's direct IP access
rustdesk://192.168.1.9:21200
```

Type one in the address bar, or give it to `gliff` or to `--control`:

```
gliff rdp://ann@office-pc
gliff --control right:rustdesk://laptop
```

A tab shows the machine's screen; the mouse button in the header drives it
from the edge of this computer's screens you pick, with no window. Sound,
the clipboard and "Use as a screen" need gliff on the other machine.

## Logging in

gliff asks for the password the first time and, unless told not to, keeps it
in the desktop's keyring through the Secret Service (gnome-keyring, KeePassXC
or KWallet), under the machine's name, once the machine has taken it: a
mistyped password is never kept. A refused password is asked for
again; driving from the edge stops after one refusal rather than retry and
lock the account. Forgetting the machine's tab forgets its password. The
command-line `--control` reads the password from the keyring, so connect
from the window once first.

## Finding out what a machine runs

When a machine typed as `user@host` does not let ssh in (the connection is
refused, times out, or the key is not accepted), gliff looks for an RDP
server and for RustDesk on it, and offers to use what answers. Each answer
is checked to speak the protocol. Accepting renames the tab, keeping its
place and screen side.

## RDP

gliff uses TLS, with Network Level Authentication (NTLM) when the server
asks for it; a server that offers NLA but takes the login over TLS alone,
such as weston, gets a second try without it. Kerberos and smart cards are
not supported.

Most RDP servers show a certificate of their own making. gliff keeps the
first one it sees for each machine and refuses another after, as ssh does
with host keys; forget the tab to accept a new one. A certificate a trusted
authority signed is checked as usual.

On Windows, Remote Desktop opens a session of its own and locks the screen
at the machine itself, so driving a Windows PC from the edge works on that
session, not on what its monitor shows. Servers that share the session
already open, such as GNOME's or KDE's desktop sharing, drive the screen in
front of you.

## RustDesk

The other machine needs RustDesk's "Enable direct IP access" option and a
permanent password; gliff does not use RustDesk's rendezvous servers or IDs.
Its picture comes as VP9 or VP8, decoded with the system's libvpx.

The connection is encrypted with a key made for it, but without a
rendezvous server there is no trusted copy of the machine's identity to
check, so a machine in the middle of the network could pose as it. The
password itself never travels: only a hash, salted by the machine's own
challenge.

On Wayland, the RustDesk machine asks its user to pick the screen to share
at each connection; gliff waits up to two minutes for that, or for the user
to accept the connection when RustDesk is set to ask.

## Trying a machine without the window

```
FOREIGN_PASSWORD=… cargo run -p gliff-foreign --example foreign-check -- \
    rdp://ann@office-pc view 10 frame.ppm
```

prints what the host's ports answer, logs in, reports the screen and the
frames, moves the pointer, clicks and types "gliff", and writes the last
frame to a PPM file.
