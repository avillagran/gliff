# gliff

[![CI](https://github.com/omacom/gliff/actions/workflows/ci.yml/badge.svg)](https://github.com/omacom/gliff/actions/workflows/ci.yml)

> **This is [Andrés Villagrán](https://github.com/avillagran)'s fork of
> [omacom/gliff](https://github.com/omacom/gliff).** Its `main` is gliff's
> `main` plus the pull requests below, which are open on gliff and not part
> of it yet, so they can be used today. Each one lives on its own branch,
> based on gliff's `main`, and is what gets reviewed upstream; once they are
> merged, this fork follows gliff again.
>
> | Pull request | What it adds |
> | --- | --- |
> | [omacom/gliff#99](https://github.com/omacom/gliff/pull/99) | Offers to install gliff on a remote that lacks it |
> | [omacom/gliff#100](https://github.com/omacom/gliff/pull/100) | Leaves fullscreen when the shown connection drops, and returns on reconnect |
> | [omacom/gliff#101](https://github.com/omacom/gliff/pull/101) | `gliff --stdio` and `--fullscreen`: show a server whose stream arrives on stdin and stdout |
> | [omacom/gliff#102](https://github.com/omacom/gliff/pull/102) | `gliff-server --position` and `--align`: place a headless output beside the other screens |
> | [omacom/gliff#103](https://github.com/omacom/gliff/pull/103) | Use another machine's screen as one more screen of this one |
> | [omacom/gliff#104](https://github.com/omacom/gliff/pull/104) | Decodes on the CPU when the GPU decodes nothing (NVIDIA's VA-API) |
> | [omacom/gliff#105](https://github.com/omacom/gliff/pull/105) | `gliff --control`: one mouse and keyboard for several machines, across screen edges |
> | [omacom/gliff#106](https://github.com/omacom/gliff/pull/106) | Control the selected machine from the window, hide it in the tray, and hear that machine here |
> | [omacom/gliff#108](https://github.com/omacom/gliff/pull/108) | Offers this computer's `gliff-server` to a machine too old to be controlled |
>
> Only here, not sent to gliff yet: `gliff-server --vnc`, which serves a
> screen to VNC viewers ([docs/vnc.md](docs/vnc.md)). Planned: RDP and
> RustDesk.
>
> To install this fork instead of the gliff package, on each machine:
>
> ```
> git clone https://github.com/avillagran/gliff.git
> cd gliff
> bin/install
> ```
>
> `omarchy pkg add gliff` (or `sudo pacman -S gliff`) goes back to gliff's
> own package.
>
> **Another machine as one more screen.** Select a machine's tab and press
> the display button in the title bar. The first time, it asks on which
> side of this computer's screens that machine's screen stands and how it
> lines up (the gear button changes it later). A headless output appears
> there, the other machine shows it fullscreen, and its keyboard and mouse
> control the apps on it. Press the button again, or close either window,
> to give the screen back. Only this machine needs ssh access to the other
> one. The other machine needs gliff too. This fork's gliff there takes the
> stream itself (`gliff --stdio`); gliff 0.4.1 or older still works,
> through a relay that Hyprland makes fullscreen.
>
> ![Choosing the side of the other machine's screen](.github/assets/screenshots/screen-settings.png)
>
> ![A headless output beside this machine's screen](.github/assets/screenshots/screen-placement.png)
>
> ![The window while the other machine's screen is in use](.github/assets/screenshots/screen-in-use.png)
>
> **One mouse for several machines.** Select a machine's tab and press the
> mouse button in the title bar. The first time, it asks on which side that
> machine stands. Push the pointer past that edge and your keyboard follows
> it; cross back to return. The speaker button brings that machine's sound
> here, and the hide button leaves an icon in the tray.
>
> ![Choosing the side of the machine to control](.github/assets/screenshots/control-settings.png)
>
> ![Controlling the selected machine](.github/assets/screenshots/control-driving.png)
>
> ![The tray icon, second from the left](.github/assets/screenshots/control-tray.png)
>
> If that machine's `gliff-server` is too old for this, gliff says so and
> offers to install this computer's server for your user, in `~/.local/bin`.
> For another architecture, such as an aarch64 laptop and an x86_64 desktop,
> keep a server for it at `~/.local/share/gliff/servers/<arch>/gliff-server`.
>
> ![The prompt for a machine too old to control](.github/assets/screenshots/update-prompt.png)

gliff is a remote desktop for Hyprland. It shows the Hyprland session of
another machine in a window on yours, and all it needs between the two is
ssh: no open ports, no accounts, no relay.

Text stays sharp. Most remote desktops send video with halved colour
resolution, which blurs coloured text; gliff sends full-resolution colour
(4:4:4) through the H.264 hardware every GPU has. Video runs on the GPU at
both ends, and falls back to the CPU on machines where it cannot.

![A gliff window showing a terminal on a remote machine](.github/assets/screenshot.png)

## What you get

- Mirror any screen of the remote machine, or get a private screen that
  exists only for your session and follows the size of your window.
- Your keyboard layout, not the remote's: keys map the same on both ends.
- A shared clipboard in both directions: text, images, and copied files.
- Quality that adapts to the link, giving up frame rate before sharpness.
- Reconnection when the link drops.
- A window that follows your Omarchy theme.

## Requirements

- Hyprland on both machines, with a session running on the remote one.
- ssh access from your machine to the remote one.
- For GPU video: an AMD or Intel GPU with its Vulkan and VA-API drivers
  (on Arch, `vulkan-radeon` and `mesa` for AMD, or `vulkan-intel` and
  `intel-media-driver` for Intel). Without them, and on NVIDIA, gliff uses
  the CPU and needs nothing extra.

## Install

Install gliff on both machines: your machine runs `gliff`, the remote one
runs `gliff-server`.

Gliff is currently available on the **edge** channel of the [Omarchy Package
Repository (OPR)](https://github.com/omacom/omarchy-pkgs):

```
omarchy pkg add gliff
```

Then open **Gliff** from the app launcher, or run `gliff` in a terminal.

If your channel reports `target not found: gliff`, or you use Arch without
OPR, build and install the package from source:

```
git clone https://github.com/omacom/gliff.git
cd gliff
bin/install
```

This uses the current checkout, including fixes newer than the last release.

If the remote machine does not have gliff yet, connecting to it offers to
install it there for your user. Over the same ssh, the remote downloads the
gliff package from its Omarchy repository, checks it against the repository
checksum and the signature its own pacman keyring trusts, and copies it to
`~/.local`; no password or sudo is needed. A file in `~/.local` that gliff
did not put there is never replaced. To install system-wide instead, run
`omarchy pkg add gliff` or `sudo pacman -S gliff` on that machine.

To build from source, see [docs/development.md](docs/development.md).

## Use

```
gliff user@host
```

This mirrors the screen that has focus on the remote machine. To pick a
screen, or to get a private one:

```
gliff --output DP-1 user@host   # mirror a named remote screen
gliff --headless user@host      # a private remote screen, sized and
                                # scaled to this window
```

A mirrored screen keeps its own size and is fitted into the window. A private
screen follows the window as you resize it, and is removed when you
disconnect.

Every machine you connect to gets a tab in the title bar. Click a tab to
connect to that machine, or to switch to it: each connection keeps running
while you look at another, so switching is instant. Hover a tab for its
button: a stop square disconnects a running machine, and an X forgets one
that is not running. Drag the tabs to reorder them. The + after the tabs
opens the address bar; type `user@host` and press Enter to add a machine.
A bare `gliff` opens the window with a tab for each machine you have used,
and connects again to the ones that were connected when you closed it; with
no machines yet, it opens with the address bar ready.

**Keyboard.** While the pointer is over the picture, everything you type goes
to the remote machine, window-manager shortcuts included; move the pointer
off it to get your own back. Press `Shift+Esc` to get your shortcuts back
without moving the pointer. `--release-hotkey` changes that key (`ctrl+alt+q`,
`double-escape`, or `none`).

**Clipboard.** Copy on one machine and paste on the other. A copy announces
only its types, and the contents are sent when the other side asks for them,
normally when you paste. A paste that takes more than a second shows its
progress with a cancel button. Pasted files are kept in `~/.cache/gliff/clipboard`.

What you copy locally is offered only to the machine in the tab you are
looking at. Switching tabs withdraws it from the old machine and offers your
current clipboard to the new one.

Clipboard sharing trusts the host you are looking at: it can request the
advertised clipboard contents without a local paste. Use `gliff --no-clipboard`
to disable sharing in both directions when connecting to an untrusted host.
Local selections marked `x-kde-passwordManagerHint` are not shared.

Changing the clipboard cancels unfinished pastes from this machine to the
remote host. Wait for a file paste to finish before copying something else. Switching
tabs also cancels unfinished pastes to the tab you leave.

**Slow links.** gliff measures the link and lowers the frame rate, then the
colour detail, then the resolution, and raises them again when there is
room. Two flags change the video, and you pass them to the server through
`--server-bin`: `--low-bandwidth` sends 4:2:0 colour from the start, and
`--full-chroma` keeps 4:4:4 on a server that encodes on the CPU.

```
gliff --server-bin 'gliff-server --low-bandwidth' user@host
```

**Any device as a screen.** `gliff-server --vnc` serves a new screen of
this machine to VNC clients, so an old tablet or laptop with a VNC viewer
can stand beside your monitors as one more of them:

```
gliff-server --vnc 0.0.0.0:5900 --headless --size 1024x768 \
    --position right --vnc-password-file ~/.config/gliff/vnc-password
```

See [docs/vnc.md](docs/vnc.md) for the options, the clients it works with,
and what to know about VNC's security.

**One mouse and keyboard for several machines.** `--control SIDE:HOST`
places another machine beside this one's screens, with no window and no
video: push the pointer past that edge (`left`, `right`, `above` or `below`)
and it carries on across the other machine's screen, and the keyboard goes
with it. Cross back over the same edge, or press the release hotkey
(`Shift+Esc`, or a chord given to `--release-hotkey`), to come back. Give it
once for each machine:

```
gliff --control left:user@laptop --control right:user@desktop
```

The other machine needs a `gliff-server` that knows `--control`; the pointer
moves on its screen on the side facing this one. A machine that cannot be
reached leaves its edge inactive until gliff connects to it again. The
clipboard is not shared this way yet.

## If something does not work

- Run `gliff-probe all` on each machine. It checks the compositor, the GPU
  drivers and both video paths, and prints PASS or FAIL for each.
- If ssh cannot find `gliff-server`, give its path:
  `gliff --server-bin /path/to/gliff-server user@host`.
- The server needs the remote user's running Hyprland session, which it
  finds under `XDG_RUNTIME_DIR`, or `/run/user/<uid>` when ssh does not set
  it. If the session is elsewhere, set it through
  `--server-bin 'env XDG_RUNTIME_DIR=/path gliff-server'`.
- To rule out a driver problem, force the CPU on your end with
  `gliff --video cpu user@host`.
- If the window connects but stays black, the remote GPU may allocate
  capture buffers it never fills (a VM's virtio-gpu with virgl, or a GPU the
  compositor does not render on). Have the compositor read the frames back
  instead: `gliff --server-bin 'gliff-server --capture shm' user@host`.
  The server then encodes on the CPU.
- Screens wider than 4096 pixels are streamed at a reduced size and scaled
  back up.

## More

- [docs/architecture.md](docs/architecture.md): how it works, the design
  choices, measurements, and what is not built yet.
- [docs/hardware-quirks.md](docs/hardware-quirks.md): what each GPU driver
  and Hyprland do, and what is tested where.
- [docs/development.md](docs/development.md): building, running on one
  machine, and the tests.
- [docs/vnc.md](docs/vnc.md): serving a screen to VNC clients.

gliff is released under the MIT license.
