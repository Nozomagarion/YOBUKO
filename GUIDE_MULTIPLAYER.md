# Playing together in YOBUKO: first session

YOBUKO connects **over the Internet with a simple code**: the host creates a session, sends the
code (on Discord or any messaging app) and the others type it to join, even hundreds of
kilometres away. No account, no router settings. **One host and up to seven guests.** Each
person listens and starts playback on their own computer. Supported edits are confirmed by the
host; audio is not streamed live. A local-network mode is still available.

## Prepare the computers

Install **the same version** on every computer, with the installer for each system (Mac,
Windows and Linux have different files) from the
[Releases page](https://github.com/Nozomagarion/YOBUKO/releases). Don't compare only the generic
number "0.1.0": an older build can be refused even if its interface looks identical. Keep your
usual projects and a backup copy. The new version reads older projects; its new saves use format
v4.

Each computer just needs Internet. Choose the audio output in the settings. If the system asks
for a network permission for YOBUKO, accept it. Don't put settings or projects inside the
application's folder.

## Connect for the first time

1. On the host computer, open YOBUKO, then **Multiplayer** at the top right of the window. Leave
   **Over the Internet** selected. No personal project is needed for this first try.
2. Click **Create session**. YOBUKO goes online (a few seconds), then a four-track demo copy
   opens in a new shared tab. Projects that were already open stay open.
3. After a few seconds a **short code** appears, for example `7-piano-tigre-lune`: a number and
   three words. Read it out (on Discord, for example) or click **Copy code** to send it
   privately. **The same code works for the whole group**: each friend types it, for thirty
   minutes, up to seven guests.
4. On another computer, open **Multiplayer**, type the code in **Invitation code from the host**
   and click **Join session**. Capitals, accents and spaces instead of dashes are accepted; a
   misspelt word is pointed out at once. Wait for the shared tab to open (up to about twenty
   seconds the first time). **Guests: N / 7** confirms who is there.
5. The other friends type the same code. **New invitation** is only needed if the code reached
   someone who shouldn't have it: it makes a new code and the old one stops working (people
   already connected stay). Eight people at most in total: the host and seven guests.

**Short code or full invitation?** The short code goes through no account and no server of ours:
the host opens a small "waiting room" whose address is derived from the code and published by
iroh's public directory. The guest finds it with the code, proves they know it, receives the full
invitation and joins the session. Guessing a code within thirty minutes is out of reach (about 43
bits, and each guess costs computation); the host sees every guest arrive. If the short code
fails (for example a company or school network that blocks that directory), the host clicks
**Copy full invitation** and sends the long `suhina-net://…` text: it is pasted in the same place
and always works.

Computers talk to each other directly through home routers. When a router blocks that, a free
public relay forwards the encrypted data without being able to read it. For a local network
without Internet, select **On this local network** and enter the host's private address followed
by `:0` in **Host address**.

## Check that you are working together

On one computer, rename a track with **Rename** and see the same name on the other. Try **Set
volume** and **Set pan**. In **MIDI notes**, choose a clip and change a note with **Apply note**;
the host confirms the change. Each person can click **Play locally** or **Stop locally** without
moving the other's playback.

To hear the same thing at the same time, tick **Link playback** (any participant can). The
buttons become **Play together**, **Stop together** and **Everyone from beat 16**: each click
starts, stops or moves playback on every computer at once. The start is scheduled about 0.3
seconds later so everyone starts together; audio is still not streamed, each computer plays its
own copy. Keep the shared tab in front. Unticking the box makes playback independent again.

If two edits start from the same state, one may be refused: read the message, refresh the draft
and redo the action. No refusal disappears before it can be read; **Dismiss multiplayer message**
acknowledges it. A refusal does not mean the project changed. Wait for confirmation before
chaining changes.

## Share your track and your takes

After the demo, use **Share current MIDI project** or **Share project + audio** from an open
Studio project. This shares a copy and keeps the original. Up to 64 tracks are accepted.

With **Share project + audio** (and online), all built-in effects and instruments, groups and
**CLAP, VST3 and VST2** plug-ins are shared **with their settings**. The plug-in itself does not
travel: each computer uses the plug-ins installed on it. If a guest lacks a plug-in, YOBUKO shows
it under **Plug-ins missing on this computer** (name, format, track): that plug-in stays silent
for them, but its settings are kept. Installing the same plug-in and joining again is enough.
Together you can adjust the parameters of every built-in effect (**Devices** page), but not yet a
third-party plug-in's knobs. A Sampler with a sample, or a NAM with a file, is refused for now:
those files don't travel yet.

The audio option transfers the sounds already loaded in the Studio, up to 4096 sounds, 2 GiB per
sound and 16 GiB in total. Heavy projects are supported: sounds travel in pieces checked one by
one, without being loaded whole into memory during the transfer. Each computer keeps received
sounds in its **Sound cache** folder (up to 20 GiB, oldest removed first): a sound already
received is never sent again, even in a later session, and an interrupted transfer resumes where
it stopped. When more than 32 MiB of sounds are missing, the copy opens at once and the sounds
arrive afterwards; their clips stay silent until then. Audio copies are kept in the **Shared
projects** subfolder of the application's data folder.

For a new take, record and finish it in a local Studio tab, then open **Publish take**, choose the
loaded sound, the track and the position, and use **Publish loaded take**. Guests can publish
too. The whole sound is sent; local trims and effects are not baked in. Recording directly into
the shared tab is not provided.

## Leave and come back

**Leave session** keeps the last copy received and makes it editable locally. A guest leaving
leaves the others connected. To come back, ask for a new invitation: a new copy opens in another
tab, without merging older local edits. The host leaving closes the connections; participants'
copies are kept. Save the copies you want to keep as usual.

**Connection drops.** If a guest's Internet or Wi-Fi drops, their status shows
**Reconnecting…** and YOBUKO retries by itself for about a minute, without a new code. Edits made
meanwhile are sent as soon as the connection is back, and an edit in flight when it dropped is
applied only once. The host sees "guest(s) reconnecting" and keeps the guest's seat for two
minutes. If the host ends the session on purpose, the guest reads "The host ended the session"
and YOBUKO does not retry. After a minute without success, "Lost the connection to the host" asks
for a new code; the copy is kept.

**Refusal messages.** A refusal says why: different versions ("Your partner runs a different
Suhina build (host abc1234, this computer def5678)": install the same version on both
computers), code expired after thirty minutes, code replaced by a newer one or mistyped, or
session full (seven guests). In every case ask the host for a new code, except for a version
difference.

## Versions and help

**Settings → Agents and updates → GitHub updates…** compares the sources and looks for a combined
package. A new official Suhina revision may need integration first: this button never replaces
YOBUKO with the official Suhina app. All participants must move to the same new version.

The exact delivery state, checks and physical tests still to do are in
[PROGRESS.md](PROGRESS.md). Handing the host role to someone else is a later extension; resuming
after a drop has so far been tested only with simulated drops.
