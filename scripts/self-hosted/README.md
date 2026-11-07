# Building on the owner's own machines

YOBUKO's workflows use no GitHub-hosted runners: every job runs on one of the owner's computers,
registered with GitHub as a self-hosted runner.

| Label            | Machine                                   | Builds                                    |
| ---------------- | ----------------------------------------- | ----------------------------------------- |
| `yobuko-mac`     | the owner's Mac                           | tests, `YOBUKO-Mac.dmg`                   |
| `yobuko-windows` | the owner's Windows PC                    | tests, `YOBUKO-Windows-Setup.exe`         |
| `yobuko-linux`   | the same PC, Ubuntu 22.04 in WSL (2 runners) | tests, AppImage, `.deb`, release publish |

## Setting up the Linux runner

On the Windows PC, double-click `BRANCHER-YOBUKO-LINUX.bat` and accept the administrator prompt.
It installs WSL (a restart is asked the first time; then double-click again), imports a dedicated
Ubuntu 22.04 named `YOBUKO-Linux`, opens the GitHub page with the registration token to paste,
installs the build tools, Rust and the GitHub CLI, registers `yobuko-linux-1` and
`yobuko-linux-2`, and adds the Windows task `YOBUKO Linux runner` that starts them at each
sign-in. Running it again updates everything.

Ubuntu 22.04 is kept on purpose: the Linux packages need the glibc of the system they are built
on, or newer, so building on 22.04 lets them run on distributions as old as it.
